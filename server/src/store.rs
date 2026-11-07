//! Acesso ao PostgreSQL (sqlx). Todas as consultas são parametrizadas.
use crate::ids::ts_ms;
use sqlx::{postgres::PgPoolOptions, PgPool};
use std::time::Duration;
use telacast_proto::{Message, Reply};

pub type R<T> = Result<T, sqlx::Error>;

#[derive(Clone)]
pub struct Store {
    pub pool: PgPool,
}

pub enum RegisterError {
    InvalidInvite,
    Taken,
    Db(sqlx::Error),
}

impl From<sqlx::Error> for RegisterError {
    fn from(e: sqlx::Error) -> Self {
        RegisterError::Db(e)
    }
}

const SESSION_DAYS: &str = "30 days";

impl Store {
    /// Conecta e aplica as migrações embutidas no binário.
    pub async fn connect(url: &str) -> R<Self> {
        let pool = PgPoolOptions::new()
            .max_connections(10)
            .acquire_timeout(Duration::from_secs(5))
            .connect(url)
            .await?;
        sqlx::migrate!("./migrations").run(&pool).await.map_err(|e| sqlx::Error::Migrate(Box::new(e)))?;
        Ok(Self { pool })
    }

    // ---------------------------------------------------------------- contas

    /// Consome um uso do convite de instância e cria o usuário, tudo numa transação.
    pub async fn register(
        &self,
        id: i64,
        username: &str,
        pass_hash: &str,
        invite_hash: &[u8],
    ) -> Result<i64, RegisterError> {
        let mut tx = self.pool.begin().await?;
        let ok: Option<(i64,)> = sqlx::query_as(
            "UPDATE instance_invites SET uses = uses + 1
             WHERE code_hash = $1 AND revoked_at IS NULL
               AND (expires_at IS NULL OR expires_at > now()) AND uses < max_uses
             RETURNING id",
        )
        .bind(invite_hash)
        .fetch_optional(&mut *tx)
        .await?;
        if ok.is_none() {
            return Err(RegisterError::InvalidInvite); // tx descartada: nada foi consumido
        }
        let r = sqlx::query("INSERT INTO users(id, username, display_name, pass_hash) VALUES ($1, $2, $2, $3)")
            .bind(id)
            .bind(username)
            .bind(pass_hash)
            .execute(&mut *tx)
            .await;
        match r {
            Ok(_) => {}
            Err(sqlx::Error::Database(e)) if e.is_unique_violation() => return Err(RegisterError::Taken),
            Err(e) => return Err(RegisterError::Db(e)),
        }
        tx.commit().await?;
        Ok(id)
    }

    pub async fn create_instance_invite(
        &self,
        id: i64,
        code_hash: &[u8],
        label: Option<&str>,
        max_uses: i32,
        days: Option<i32>,
    ) -> R<()> {
        sqlx::query(
            "INSERT INTO instance_invites(id, code_hash, label, max_uses, expires_at)
             VALUES ($1, $2, $3, $4, CASE WHEN $5::int IS NULL THEN NULL ELSE now() + make_interval(days => $5::int) END)",
        )
        .bind(id)
        .bind(code_hash)
        .bind(label)
        .bind(max_uses)
        .bind(days)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// (id, hash da senha, nome de exibição)
    pub async fn user_by_username(&self, username: &str) -> R<Option<(i64, String, String)>> {
        sqlx::query_as("SELECT id, pass_hash, display_name FROM users WHERE lower(username) = lower($1)")
            .bind(username)
            .fetch_optional(&self.pool)
            .await
    }

    pub async fn find_user_id(&self, username: &str) -> R<Option<i64>> {
        let r: Option<(i64,)> = sqlx::query_as("SELECT id FROM users WHERE lower(username) = lower($1)")
            .bind(username)
            .fetch_optional(&self.pool)
            .await?;
        Ok(r.map(|x| x.0))
    }

    pub async fn set_status(&self, uid: i64, status: i16) -> R<()> {
        sqlx::query("UPDATE users SET status = $2 WHERE id = $1").bind(uid).bind(status).execute(&self.pool).await?;
        Ok(())
    }

    /// Status escolhido (0 a 3) de cada usuário da lista.
    pub async fn manual_statuses(&self, ids: &[i64]) -> R<Vec<(i64, i16)>> {
        sqlx::query_as("SELECT id, status FROM users WHERE id = ANY($1)").bind(ids).fetch_all(&self.pool).await
    }

    pub async fn display_name(&self, id: i64) -> R<Option<String>> {
        let r: Option<(String,)> = sqlx::query_as("SELECT display_name FROM users WHERE id = $1")
            .bind(id)
            .fetch_optional(&self.pool)
            .await?;
        Ok(r.map(|x| x.0))
    }

    // --------------------------------------------------------------- sessões

    pub async fn create_session(&self, id: i64, user_id: i64, token_hash: &[u8]) -> R<()> {
        sqlx::query(
            "INSERT INTO sessions(id, user_id, token_hash, expires_at)
             VALUES ($1, $2, $3, now() + $4::interval)",
        )
        .bind(id)
        .bind(user_id)
        .bind(token_hash)
        .bind(SESSION_DAYS)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Valida a sessão e renova a validade (janela deslizante).
    pub async fn session_user(&self, token_hash: &[u8]) -> R<Option<i64>> {
        let r: Option<(i64,)> = sqlx::query_as(
            "UPDATE sessions SET last_used_at = now(), expires_at = now() + $2::interval
             WHERE token_hash = $1 AND expires_at > now() RETURNING user_id",
        )
        .bind(token_hash)
        .bind(SESSION_DAYS)
        .fetch_optional(&self.pool)
        .await?;
        Ok(r.map(|x| x.0))
    }

    pub async fn revoke_session(&self, token_hash: &[u8]) -> R<()> {
        sqlx::query("DELETE FROM sessions WHERE token_hash = $1").bind(token_hash).execute(&self.pool).await?;
        Ok(())
    }

    // ------------------------------------------------------------- servidores

    pub async fn create_server(&self, id: i64, category_id: i64, channel_id: i64, owner: i64, name: &str, invite: &str) -> R<()> {
        let mut tx = self.pool.begin().await?;
        sqlx::query("INSERT INTO servers(id, name, owner_id) VALUES ($1, $2, $3)")
            .bind(id)
            .bind(name)
            .bind(owner)
            .execute(&mut *tx)
            .await?;
        sqlx::query("INSERT INTO members(server_id, user_id) VALUES ($1, $2)")
            .bind(id)
            .bind(owner)
            .execute(&mut *tx)
            .await?;
        sqlx::query("INSERT INTO channels(id, server_id, kind, name, position) VALUES ($1, $2, 4, 'canais de texto', 0)")
            .bind(category_id)
            .bind(id)
            .execute(&mut *tx)
            .await?;
        sqlx::query("INSERT INTO channels(id, server_id, parent_id, kind, name, position) VALUES ($1, $2, $3, 0, 'geral', 1)")
            .bind(channel_id)
            .bind(id)
            .bind(category_id)
            .execute(&mut *tx)
            .await?;
        sqlx::query("INSERT INTO server_invites(code, server_id, created_by) VALUES ($1, $2, $3)")
            .bind(invite)
            .bind(id)
            .bind(owner)
            .execute(&mut *tx)
            .await?;
        tx.commit().await
    }

    /// Entra no servidor pelo código. `None` se o convite for inválido, expirado, revogado ou esgotado.
    pub async fn join_by_invite(&self, code: &str, user: i64) -> R<Option<i64>> {
        let mut tx = self.pool.begin().await?;
        let sid: Option<(i64,)> = sqlx::query_as(
            "SELECT server_id FROM server_invites
             WHERE code = $1 AND revoked_at IS NULL
               AND (expires_at IS NULL OR expires_at > now())
               AND (max_uses IS NULL OR uses < max_uses)
             FOR UPDATE",
        )
        .bind(code)
        .fetch_optional(&mut *tx)
        .await?;
        let Some((sid,)) = sid else { return Ok(None) };
        let already: Option<(i32,)> = sqlx::query_as("SELECT 1 FROM members WHERE server_id = $1 AND user_id = $2")
            .bind(sid)
            .bind(user)
            .fetch_optional(&mut *tx)
            .await?;
        if already.is_none() {
            sqlx::query("INSERT INTO members(server_id, user_id) VALUES ($1, $2)")
                .bind(sid)
                .bind(user)
                .execute(&mut *tx)
                .await?;
            sqlx::query("UPDATE server_invites SET uses = uses + 1 WHERE code = $1").bind(code).execute(&mut *tx).await?;
        }
        tx.commit().await?;
        Ok(Some(sid))
    }

    pub async fn server_owner(&self, server: i64) -> R<Option<i64>> {
        let r: Option<(i64,)> = sqlx::query_as("SELECT owner_id FROM servers WHERE id = $1")
            .bind(server)
            .fetch_optional(&self.pool)
            .await?;
        Ok(r.map(|x| x.0))
    }

    /// Cria canal de texto (kind 0) ou categoria (kind 4). Devolve false se a categoria-pai
    /// não existe neste servidor.
    pub async fn create_channel(&self, id: i64, server: i64, name: &str, kind: i16, parent: Option<i64>) -> R<bool> {
        let r = sqlx::query(
            "INSERT INTO channels(id, server_id, parent_id, kind, name, position)
             SELECT $1, $2, $5, $4, $3, COALESCE((SELECT max(position) + 1 FROM channels WHERE server_id = $2), 0)
             WHERE $5::bigint IS NULL
                OR EXISTS (SELECT 1 FROM channels WHERE id = $5 AND server_id = $2 AND kind = 4)",
        )
        .bind(id)
        .bind(server)
        .bind(name)
        .bind(kind)
        .bind(parent)
        .execute(&self.pool)
        .await?;
        Ok(r.rows_affected() == 1)
    }

    pub async fn server_member_ids(&self, server: i64) -> R<Vec<i64>> {
        let r: Vec<(i64,)> = sqlx::query_as("SELECT user_id FROM members WHERE server_id = $1")
            .bind(server)
            .fetch_all(&self.pool)
            .await?;
        Ok(r.into_iter().map(|x| x.0).collect())
    }

    // -------------------------------------------------------------- acesso

    /// Quem pode ver o canal: membros do servidor, ou os participantes da DM.
    pub async fn participants(&self, channel: i64) -> R<Vec<i64>> {
        let r: Vec<(i64,)> = sqlx::query_as(
            "SELECT m.user_id FROM channels c JOIN members m ON m.server_id = c.server_id WHERE c.id = $1
             UNION
             SELECT user_id FROM channel_members WHERE channel_id = $1",
        )
        .bind(channel)
        .fetch_all(&self.pool)
        .await?;
        Ok(r.into_iter().map(|x| x.0).collect())
    }

    pub async fn can_access(&self, channel: i64, user: i64) -> R<bool> {
        let r: (bool,) = sqlx::query_as(
            "SELECT EXISTS (SELECT 1 FROM channels c JOIN members m ON m.server_id = c.server_id
                            WHERE c.id = $1 AND m.user_id = $2 AND c.kind = 0)
                 OR EXISTS (SELECT 1 FROM channel_members WHERE channel_id = $1 AND user_id = $2)",
        )
        .bind(channel)
        .bind(user)
        .fetch_one(&self.pool)
        .await?;
        Ok(r.0)
    }

    // ------------------------------------------------------------- mensagens

    /// `Ok(None)` se `reply_to` não é uma mensagem deste canal.
    pub async fn insert_message(
        &self,
        id: i64,
        channel: i64,
        author: i64,
        content: &str,
        reply_to: Option<i64>,
    ) -> R<Option<Message>> {
        let reply = match reply_to {
            None => None,
            Some(r) => {
                let row: Option<(i64, String, String)> = sqlx::query_as(
                    "SELECT m.id, u.display_name, left(m.content, 120) FROM messages m
                     JOIN users u ON u.id = m.author_id WHERE m.id = $1 AND m.channel_id = $2",
                )
                .bind(r)
                .bind(channel)
                .fetch_optional(&self.pool)
                .await?;
                match row {
                    Some((id, author, content)) => Some(Reply { id, author, content }),
                    None => return Ok(None),
                }
            }
        };
        let (name,): (String,) = sqlx::query_as(
            "WITH ins AS (INSERT INTO messages(id, channel_id, author_id, content, reply_to)
                          VALUES ($1, $2, $3, $4, $5) RETURNING author_id)
             SELECT u.display_name FROM users u JOIN ins ON ins.author_id = u.id",
        )
        .bind(id)
        .bind(channel)
        .bind(author)
        .bind(content)
        .bind(reply_to)
        .fetch_one(&self.pool)
        .await?;
        Ok(Some(Message {
            id,
            channel_id: channel,
            author_id: author,
            author: name,
            content: content.to_string(),
            ts: ts_ms(id),
            edited: false,
            reply,
            nonce: 0,
        }))
    }

    /// Devolve o canal da mensagem editada, ou `None` se não existe ou não é do autor.
    pub async fn edit_message(&self, id: i64, author: i64, content: &str) -> R<Option<i64>> {
        let r: Option<(i64,)> = sqlx::query_as(
            "UPDATE messages SET content = $3, edited_at = now() WHERE id = $1 AND author_id = $2 RETURNING channel_id",
        )
        .bind(id)
        .bind(author)
        .bind(content)
        .fetch_optional(&self.pool)
        .await?;
        Ok(r.map(|x| x.0))
    }

    /// Apaga se `user` é o autor ou o dono do servidor. Devolve o canal.
    pub async fn delete_message(&self, id: i64, user: i64) -> R<Option<i64>> {
        let r: Option<(i64,)> = sqlx::query_as(
            "DELETE FROM messages m WHERE m.id = $1 AND (m.author_id = $2 OR EXISTS (
                 SELECT 1 FROM channels c JOIN servers s ON s.id = c.server_id
                 WHERE c.id = m.channel_id AND s.owner_id = $2)) RETURNING m.channel_id",
        )
        .bind(id)
        .bind(user)
        .fetch_optional(&self.pool)
        .await?;
        Ok(r.map(|x| x.0))
    }

    /// Conta a menção para quem foi citado (`<@id>`) ou para todos (`@everyone`/`@here`), menos o autor.
    pub async fn add_mentions(&self, channel: i64, author: i64, content: &str) -> R<()> {
        let (ids, everyone) = mention_targets(content);
        if ids.is_empty() && !everyone {
            return Ok(());
        }
        sqlx::query(
            "INSERT INTO read_state(user_id, channel_id, last_read_id, mention_count)
             SELECT p.user_id, $1, 0, 1 FROM (
                 SELECT m.user_id FROM channels c JOIN members m ON m.server_id = c.server_id WHERE c.id = $1
                 UNION SELECT user_id FROM channel_members WHERE channel_id = $1
             ) p
             WHERE p.user_id <> $2 AND (p.user_id = ANY($3) OR $4)
             ON CONFLICT (user_id, channel_id) DO UPDATE SET mention_count = read_state.mention_count + 1",
        )
        .bind(channel)
        .bind(author)
        .bind(&ids)
        .bind(everyone)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn is_member(&self, server: i64, user: i64) -> R<bool> {
        let r: (bool,) = sqlx::query_as("SELECT EXISTS (SELECT 1 FROM members WHERE server_id = $1 AND user_id = $2)")
            .bind(server)
            .bind(user)
            .fetch_one(&self.pool)
            .await?;
        Ok(r.0)
    }

    /// Busca por texto (`text` vazio = só o filtro de autor). Devolve até 26 resultados (um a mais
    /// que a página, para saber se há mais), do mais novo para o mais antigo.
    pub async fn search(
        &self,
        channel: Option<i64>,
        server: Option<i64>,
        from: Option<i64>,
        text: &str,
        before: Option<i64>,
    ) -> R<Vec<(Message, String)>> {
        type Row = (i64, i64, String, i64, String, String, bool);
        let rows: Vec<Row> = sqlx::query_as(
            "SELECT m.id, m.channel_id, c.name, m.author_id, u.display_name, m.content, m.edited_at IS NOT NULL
             FROM messages m
             JOIN channels c ON c.id = m.channel_id
             JOIN users u ON u.id = m.author_id
             WHERE (($1::bigint IS NOT NULL AND m.channel_id = $1) OR ($2::bigint IS NOT NULL AND c.server_id = $2 AND c.kind = 0))
               AND ($3::bigint IS NULL OR m.author_id = $3)
               AND ($4 = '' OR to_tsvector('portuguese', m.content) @@ websearch_to_tsquery('portuguese', $4))
               AND ($5::bigint IS NULL OR m.id < $5)
             ORDER BY m.id DESC LIMIT 26",
        )
        .bind(channel)
        .bind(server)
        .bind(from)
        .bind(text)
        .bind(before)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|(id, channel_id, channel_name, author_id, author, content, edited)| {
                (
                    Message { id, channel_id, author_id, author, content, ts: ts_ms(id), edited, reply: None, nonce: 0 },
                    channel_name,
                )
            })
            .collect())
    }

    pub async fn ack(&self, user: i64, channel: i64, message: i64) -> R<()> {
        sqlx::query(
            "INSERT INTO read_state(user_id, channel_id, last_read_id) VALUES ($1, $2, $3)
             ON CONFLICT (user_id, channel_id) DO UPDATE
             SET last_read_id = GREATEST(read_state.last_read_id, EXCLUDED.last_read_id), mention_count = 0",
        )
        .bind(user)
        .bind(channel)
        .bind(message)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Até 50 mensagens anteriores a `before`, em ordem cronológica.
    pub async fn history(&self, channel: i64, before: Option<i64>) -> R<Vec<Message>> {
        type Row = (i64, i64, String, String, bool, Option<i64>, Option<String>, Option<String>);
        let rows: Vec<Row> = sqlx::query_as(
            "SELECT m.id, m.author_id, u.display_name, m.content, m.edited_at IS NOT NULL,
                    r.id, ru.display_name, left(r.content, 120)
             FROM messages m JOIN users u ON u.id = m.author_id
             LEFT JOIN messages r ON r.id = m.reply_to
             LEFT JOIN users ru ON ru.id = r.author_id
             WHERE m.channel_id = $1 AND m.id < $2
             ORDER BY m.id DESC LIMIT 50",
        )
        .bind(channel)
        .bind(before.unwrap_or(i64::MAX))
        .fetch_all(&self.pool)
        .await?;
        let mut v: Vec<Message> = rows
            .into_iter()
            .map(|(id, author_id, author, content, edited, rid, rauthor, rcontent)| Message {
                id,
                channel_id: channel,
                author_id,
                author,
                content,
                ts: ts_ms(id),
                edited,
                reply: match (rid, rauthor, rcontent) {
                    (Some(id), Some(author), Some(content)) => Some(Reply { id, author, content }),
                    _ => None,
                },
                nonce: 0,
            })
            .collect();
        v.reverse();
        Ok(v)
    }

    // --------------------------------------------------------------- amizades

    /// Pedido de `a` para `b`. Se `b` já tinha pedido a `a`, vira amizade.
    pub async fn friend_request(&self, a: i64, b: i64) -> R<()> {
        let mut tx = self.pool.begin().await?;
        let accepted = sqlx::query("UPDATE friendships SET accepted = true WHERE requester_id = $2 AND target_id = $1")
            .bind(a)
            .bind(b)
            .execute(&mut *tx)
            .await?
            .rows_affected();
        if accepted == 0 {
            sqlx::query(
                "INSERT INTO friendships(requester_id, target_id) VALUES ($1, $2) ON CONFLICT DO NOTHING",
            )
            .bind(a)
            .bind(b)
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await
    }

    /// `me` aceita o pedido que `other` enviou.
    pub async fn friend_accept(&self, me: i64, other: i64) -> R<()> {
        sqlx::query("UPDATE friendships SET accepted = true WHERE requester_id = $2 AND target_id = $1")
            .bind(me)
            .bind(other)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn friend_remove(&self, me: i64, other: i64) -> R<()> {
        sqlx::query(
            "DELETE FROM friendships
             WHERE (requester_id = $1 AND target_id = $2) OR (requester_id = $2 AND target_id = $1)",
        )
        .bind(me)
        .bind(other)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn are_friends(&self, a: i64, b: i64) -> R<bool> {
        let r: (bool,) = sqlx::query_as(
            "SELECT EXISTS (SELECT 1 FROM friendships WHERE accepted
                            AND ((requester_id = $1 AND target_id = $2) OR (requester_id = $2 AND target_id = $1)))",
        )
        .bind(a)
        .bind(b)
        .fetch_one(&self.pool)
        .await?;
        Ok(r.0)
    }

    /// (outro usuário, nome, aceito, fui eu quem pediu)
    pub async fn friends_of(&self, me: i64) -> R<Vec<(i64, String, bool, bool)>> {
        sqlx::query_as(
            "SELECT u.id, u.display_name, f.accepted, (f.requester_id = $1)
             FROM friendships f
             JOIN users u ON u.id = CASE WHEN f.requester_id = $1 THEN f.target_id ELSE f.requester_id END
             WHERE f.requester_id = $1 OR f.target_id = $1
             ORDER BY lower(u.display_name)",
        )
        .bind(me)
        .fetch_all(&self.pool)
        .await
    }

    // ------------------------------------------------------------------- DMs

    pub async fn open_dm(&self, new_id: i64, a: i64, b: i64) -> R<i64> {
        let mut tx = self.pool.begin().await?;
        let key = format!("dm:{}:{}", a.min(b), a.max(b));
        // Serializa a criação para o mesmo par (evita DMs duplicadas).
        sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 0))").bind(key).execute(&mut *tx).await?;
        let existing: Option<(i64,)> = sqlx::query_as(
            "SELECT c1.channel_id FROM channel_members c1
             JOIN channel_members c2 ON c2.channel_id = c1.channel_id
             JOIN channels ch ON ch.id = c1.channel_id
             WHERE c1.user_id = $1 AND c2.user_id = $2 AND ch.kind = 10 LIMIT 1",
        )
        .bind(a)
        .bind(b)
        .fetch_optional(&mut *tx)
        .await?;
        if let Some((id,)) = existing {
            return Ok(id);
        }
        sqlx::query("INSERT INTO channels(id, kind, name) VALUES ($1, 10, 'dm')").bind(new_id).execute(&mut *tx).await?;
        for u in [a, b] {
            sqlx::query("INSERT INTO channel_members(channel_id, user_id) VALUES ($1, $2)")
                .bind(new_id)
                .bind(u)
                .execute(&mut *tx)
                .await?;
        }
        tx.commit().await?;
        Ok(new_id)
    }

    /// (canal, outro usuário, nome)
    pub async fn dms_of(&self, me: i64) -> R<Vec<(i64, i64, String, i64, i64, i32)>> {
        sqlx::query_as(
            "SELECT c1.channel_id, u.id, u.display_name,
                    COALESCE((SELECT max(m.id) FROM messages m WHERE m.channel_id = c1.channel_id), 0),
                    COALESCE((SELECT rs.last_read_id FROM read_state rs WHERE rs.user_id = $1 AND rs.channel_id = c1.channel_id), 0),
                    COALESCE((SELECT rs.mention_count FROM read_state rs WHERE rs.user_id = $1 AND rs.channel_id = c1.channel_id), 0)
             FROM channel_members c1
             JOIN channels ch ON ch.id = c1.channel_id AND ch.kind = 10
             JOIN channel_members c2 ON c2.channel_id = c1.channel_id AND c2.user_id <> $1
             JOIN users u ON u.id = c2.user_id
             WHERE c1.user_id = $1 ORDER BY c1.channel_id",
        )
        .bind(me)
        .fetch_all(&self.pool)
        .await
    }

    /// Todos que devem ser avisados quando `me` muda de estado.
    pub async fn related_users(&self, me: i64) -> R<Vec<i64>> {
        let r: Vec<(i64,)> = sqlx::query_as(
            "SELECT CASE WHEN requester_id = $1 THEN target_id ELSE requester_id END
               FROM friendships WHERE requester_id = $1 OR target_id = $1
             UNION SELECT m2.user_id FROM members m1 JOIN members m2 ON m2.server_id = m1.server_id
               WHERE m1.user_id = $1 AND m2.user_id <> $1
             UNION SELECT c2.user_id FROM channel_members c1 JOIN channel_members c2 ON c2.channel_id = c1.channel_id
               WHERE c1.user_id = $1 AND c2.user_id <> $1",
        )
        .bind(me)
        .fetch_all(&self.pool)
        .await?;
        Ok(r.into_iter().map(|x| x.0).collect())
    }

    // -------------------------------------------------------------- snapshot

    /// (id, nome, dono, convite ativo)
    pub async fn servers_of(&self, me: i64) -> R<Vec<(i64, String, i64, String)>> {
        sqlx::query_as(
            "SELECT s.id, s.name, s.owner_id,
                    COALESCE((SELECT i.code FROM server_invites i
                              WHERE i.server_id = s.id AND i.revoked_at IS NULL
                                AND (i.expires_at IS NULL OR i.expires_at > now())
                              ORDER BY i.created_at LIMIT 1), '')
             FROM servers s JOIN members m ON m.server_id = s.id
             WHERE m.user_id = $1 ORDER BY s.id",
        )
        .bind(me)
        .fetch_all(&self.pool)
        .await
    }

    /// (servidor, canal, nome, tipo, categoria, posição) dos canais de texto e categorias.
    pub async fn channels_in(&self, servers: &[i64], me: i64) -> R<Vec<(i64, i64, String, i16, Option<i64>, i32, i64, i64, i32)>> {
        sqlx::query_as(
            "SELECT c.server_id, c.id, c.name, c.kind, c.parent_id, c.position,
                    COALESCE((SELECT max(m.id) FROM messages m WHERE m.channel_id = c.id), 0),
                    COALESCE((SELECT rs.last_read_id FROM read_state rs WHERE rs.user_id = $2 AND rs.channel_id = c.id), 0),
                    COALESCE((SELECT rs.mention_count FROM read_state rs WHERE rs.user_id = $2 AND rs.channel_id = c.id), 0)
             FROM channels c
             WHERE c.server_id = ANY($1) AND c.kind IN (0, 4) ORDER BY c.server_id, c.position, c.id",
        )
        .bind(servers)
        .bind(me)
        .fetch_all(&self.pool)
        .await
    }

    /// (servidor, usuário, nome)
    pub async fn members_in(&self, servers: &[i64]) -> R<Vec<(i64, i64, String)>> {
        sqlx::query_as(
            "SELECT m.server_id, u.id, u.display_name FROM members m JOIN users u ON u.id = m.user_id
             WHERE m.server_id = ANY($1) ORDER BY lower(u.display_name)",
        )
        .bind(servers)
        .fetch_all(&self.pool)
        .await
    }

    /// Dos canais dados, os que `me` pode ver.
    pub async fn accessible_channels(&self, channels: &[i64], me: i64) -> R<Vec<i64>> {
        let r: Vec<(i64,)> = sqlx::query_as(
            "SELECT c.id FROM channels c
             WHERE c.id = ANY($1)
               AND (c.server_id IN (SELECT server_id FROM members WHERE user_id = $2)
                    OR c.id IN (SELECT channel_id FROM channel_members WHERE user_id = $2))",
        )
        .bind(channels)
        .bind(me)
        .fetch_all(&self.pool)
        .await?;
        Ok(r.into_iter().map(|x| x.0).collect())
    }
}

/// Quem o texto menciona: ids de `<@id>`/`<@!id>` (no máximo 20) e se há `@everyone`/`@here`.
pub fn mention_targets(content: &str) -> (Vec<i64>, bool) {
    let mut ids: Vec<i64> = Vec::new();
    let mut rest = content;
    while let Some(p) = rest.find("<@") {
        let after = &rest[p + 2..];
        let after = after.strip_prefix('!').unwrap_or(after);
        if let Some(end) = after.find('>') {
            if let Ok(id) = after[..end].parse::<i64>() {
                if !ids.contains(&id) && ids.len() < 20 {
                    ids.push(id);
                }
            }
        }
        rest = &rest[p + 2..];
    }
    let everyone = content.contains("@everyone") || content.contains("@here");
    (ids, everyone)
}

#[cfg(test)]
mod tests {
    use super::mention_targets;

    #[test]
    fn acha_mencoes() {
        assert_eq!(mention_targets("oi <@12> e <@!34>, <@12> de novo"), (vec![12, 34], false));
        assert_eq!(mention_targets("@everyone vamos"), (vec![], true));
        assert_eq!(mention_targets("<@abc> <@>"), (vec![], false));
        assert_eq!(mention_targets("sem nada"), (vec![], false));
    }
}
