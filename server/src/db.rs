use rusqlite::{params, Connection, OptionalExtension};
use telacast_proto::{Channel, Message};

pub struct Db(pub Connection);

pub type R<T> = rusqlite::Result<T>;

pub fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

impl Db {
    pub fn open(path: &str) -> R<Self> {
        let c = Connection::open(path)?;
        c.execute_batch(
            "PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;
             CREATE TABLE IF NOT EXISTS users(
                id INTEGER PRIMARY KEY, name TEXT NOT NULL UNIQUE COLLATE NOCASE,
                pass TEXT NOT NULL, created INTEGER NOT NULL);
             CREATE TABLE IF NOT EXISTS tokens(
                token TEXT PRIMARY KEY, user_id INTEGER NOT NULL REFERENCES users(id));
             CREATE TABLE IF NOT EXISTS servers(
                id INTEGER PRIMARY KEY, name TEXT NOT NULL, owner INTEGER NOT NULL,
                invite TEXT NOT NULL UNIQUE);
             CREATE TABLE IF NOT EXISTS members(
                server_id INTEGER NOT NULL REFERENCES servers(id),
                user_id INTEGER NOT NULL REFERENCES users(id),
                PRIMARY KEY(server_id, user_id));
             -- server_id NULL = conversa privada (DM)
             CREATE TABLE IF NOT EXISTS channels(
                id INTEGER PRIMARY KEY, server_id INTEGER REFERENCES servers(id),
                name TEXT NOT NULL);
             CREATE TABLE IF NOT EXISTS dm_members(
                channel_id INTEGER NOT NULL REFERENCES channels(id),
                user_id INTEGER NOT NULL REFERENCES users(id),
                PRIMARY KEY(channel_id, user_id));
             CREATE TABLE IF NOT EXISTS messages(
                id INTEGER PRIMARY KEY, channel_id INTEGER NOT NULL REFERENCES channels(id),
                author_id INTEGER NOT NULL REFERENCES users(id),
                content TEXT NOT NULL, ts INTEGER NOT NULL);
             CREATE INDEX IF NOT EXISTS msg_chan ON messages(channel_id, id);
             -- a = quem pediu, b = quem recebeu; accepted: 0 pendente, 1 amigos
             CREATE TABLE IF NOT EXISTS friends(
                a INTEGER NOT NULL, b INTEGER NOT NULL, accepted INTEGER NOT NULL DEFAULT 0,
                PRIMARY KEY(a, b));",
        )?;
        Ok(Self(c))
    }

    // ---- contas ----
    pub fn create_user(&self, name: &str, pass_hash: &str) -> R<i64> {
        self.0.execute(
            "INSERT INTO users(name, pass, created) VALUES(?1, ?2, ?3)",
            params![name, pass_hash, now_ms()],
        )?;
        Ok(self.0.last_insert_rowid())
    }

    pub fn user_by_name(&self, name: &str) -> R<Option<(i64, String, String)>> {
        self.0
            .query_row("SELECT id, pass, name FROM users WHERE name = ?1", [name], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?))
            })
            .optional()
    }

    pub fn user_name(&self, id: i64) -> R<Option<String>> {
        self.0
            .query_row("SELECT name FROM users WHERE id = ?1", [id], |r| r.get(0))
            .optional()
    }

    pub fn create_token(&self, user_id: i64) -> R<String> {
        let t = format!("{}{}", uuid::Uuid::new_v4().simple(), uuid::Uuid::new_v4().simple());
        self.0
            .execute("INSERT INTO tokens(token, user_id) VALUES(?1, ?2)", params![t, user_id])?;
        Ok(t)
    }

    pub fn user_by_token(&self, token: &str) -> R<Option<i64>> {
        self.0
            .query_row("SELECT user_id FROM tokens WHERE token = ?1", [token], |r| r.get(0))
            .optional()
    }

    // ---- servidores e canais ----
    pub fn create_server(&self, owner: i64, name: &str) -> R<i64> {
        let invite = uuid::Uuid::new_v4().simple().to_string()[..8].to_string();
        self.0.execute(
            "INSERT INTO servers(name, owner, invite) VALUES(?1, ?2, ?3)",
            params![name, owner, invite],
        )?;
        let sid = self.0.last_insert_rowid();
        self.0
            .execute("INSERT INTO members(server_id, user_id) VALUES(?1, ?2)", params![sid, owner])?;
        self.create_channel(sid, "geral")?;
        Ok(sid)
    }

    pub fn server_by_invite(&self, code: &str) -> R<Option<i64>> {
        self.0
            .query_row("SELECT id FROM servers WHERE invite = ?1", [code], |r| r.get(0))
            .optional()
    }

    pub fn add_member(&self, server_id: i64, user_id: i64) -> R<()> {
        self.0.execute(
            "INSERT OR IGNORE INTO members(server_id, user_id) VALUES(?1, ?2)",
            params![server_id, user_id],
        )?;
        Ok(())
    }

    pub fn server_owner(&self, server_id: i64) -> R<Option<i64>> {
        self.0
            .query_row("SELECT owner FROM servers WHERE id = ?1", [server_id], |r| r.get(0))
            .optional()
    }

    pub fn is_member(&self, server_id: i64, user_id: i64) -> R<bool> {
        Ok(self
            .0
            .query_row(
                "SELECT 1 FROM members WHERE server_id = ?1 AND user_id = ?2",
                params![server_id, user_id],
                |_| Ok(()),
            )
            .optional()?
            .is_some())
    }

    pub fn create_channel(&self, server_id: i64, name: &str) -> R<i64> {
        self.0.execute(
            "INSERT INTO channels(server_id, name) VALUES(?1, ?2)",
            params![server_id, name],
        )?;
        Ok(self.0.last_insert_rowid())
    }

    /// (id, nome, dono, convite) dos servidores do usuário.
    pub fn servers_of(&self, user_id: i64) -> R<Vec<(i64, String, i64, String)>> {
        let mut s = self.0.prepare(
            "SELECT s.id, s.name, s.owner, s.invite FROM servers s
             JOIN members m ON m.server_id = s.id WHERE m.user_id = ?1 ORDER BY s.id",
        )?;
        let rows = s.query_map([user_id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?;
        rows.collect()
    }

    pub fn channels_of(&self, server_id: i64) -> R<Vec<Channel>> {
        let mut s = self
            .0
            .prepare("SELECT id, name FROM channels WHERE server_id = ?1 ORDER BY id")?;
        let rows = s.query_map([server_id], |r| Ok(Channel { id: r.get(0)?, name: r.get(1)? }))?;
        rows.collect()
    }

    pub fn members_of(&self, server_id: i64) -> R<Vec<(i64, String)>> {
        let mut s = self.0.prepare(
            "SELECT u.id, u.name FROM users u JOIN members m ON m.user_id = u.id
             WHERE m.server_id = ?1 ORDER BY u.name COLLATE NOCASE",
        )?;
        let rows = s.query_map([server_id], |r| Ok((r.get(0)?, r.get(1)?)))?;
        rows.collect()
    }

    /// Quem pode ver o canal: membros do servidor ou os dois da DM.
    pub fn channel_participants(&self, channel_id: i64) -> R<Vec<i64>> {
        let server: Option<i64> = self
            .0
            .query_row("SELECT server_id FROM channels WHERE id = ?1", [channel_id], |r| r.get(0))
            .optional()?
            .flatten();
        let sql = if server.is_some() {
            "SELECT user_id FROM members WHERE server_id = ?1"
        } else {
            "SELECT user_id FROM dm_members WHERE channel_id = ?1"
        };
        let key = server.unwrap_or(channel_id);
        let mut s = self.0.prepare(sql)?;
        let rows = s.query_map([key], |r| r.get(0))?;
        rows.collect()
    }

    pub fn can_access(&self, channel_id: i64, user_id: i64) -> R<bool> {
        Ok(self.channel_participants(channel_id)?.contains(&user_id))
    }

    // ---- mensagens ----
    pub fn insert_message(&self, channel_id: i64, author_id: i64, content: &str) -> R<Message> {
        let ts = now_ms();
        self.0.execute(
            "INSERT INTO messages(channel_id, author_id, content, ts) VALUES(?1, ?2, ?3, ?4)",
            params![channel_id, author_id, content, ts],
        )?;
        Ok(Message {
            id: self.0.last_insert_rowid(),
            channel_id,
            author_id,
            author: self.user_name(author_id)?.unwrap_or_default(),
            content: content.to_string(),
            ts,
        })
    }

    /// Até 50 mensagens anteriores a `before`, em ordem cronológica.
    pub fn history(&self, channel_id: i64, before: Option<i64>) -> R<Vec<Message>> {
        let mut s = self.0.prepare(
            "SELECT m.id, m.author_id, u.name, m.content, m.ts FROM messages m
             JOIN users u ON u.id = m.author_id
             WHERE m.channel_id = ?1 AND m.id < ?2 ORDER BY m.id DESC LIMIT 50",
        )?;
        let rows = s.query_map(params![channel_id, before.unwrap_or(i64::MAX)], |r| {
            Ok(Message {
                id: r.get(0)?,
                channel_id,
                author_id: r.get(1)?,
                author: r.get(2)?,
                content: r.get(3)?,
                ts: r.get(4)?,
            })
        })?;
        let mut v: Vec<Message> = rows.collect::<R<_>>()?;
        v.reverse();
        Ok(v)
    }

    // ---- amizades ----
    /// Pedido de a para b. Se b já tinha pedido a a, vira amizade.
    pub fn friend_request(&self, a: i64, b: i64) -> R<()> {
        let reverse: Option<i64> = self
            .0
            .query_row("SELECT accepted FROM friends WHERE a = ?1 AND b = ?2", params![b, a], |r| r.get(0))
            .optional()?;
        if reverse.is_some() {
            self.0.execute(
                "UPDATE friends SET accepted = 1 WHERE a = ?1 AND b = ?2",
                params![b, a],
            )?;
        } else {
            self.0.execute(
                "INSERT OR IGNORE INTO friends(a, b, accepted) VALUES(?1, ?2, 0)",
                params![a, b],
            )?;
        }
        Ok(())
    }

    /// `me` aceita o pedido que `other` enviou.
    pub fn friend_accept(&self, me: i64, other: i64) -> R<()> {
        self.0.execute(
            "UPDATE friends SET accepted = 1 WHERE a = ?1 AND b = ?2",
            params![other, me],
        )?;
        Ok(())
    }

    pub fn friend_remove(&self, me: i64, other: i64) -> R<()> {
        self.0.execute(
            "DELETE FROM friends WHERE (a = ?1 AND b = ?2) OR (a = ?2 AND b = ?1)",
            params![me, other],
        )?;
        Ok(())
    }

    /// (outro usuário, nome, aceito, fui eu quem pediu)
    pub fn friends_of(&self, me: i64) -> R<Vec<(i64, String, bool, bool)>> {
        let mut s = self.0.prepare(
            "SELECT u.id, u.name, f.accepted, (f.a = ?1) FROM friends f
             JOIN users u ON u.id = CASE WHEN f.a = ?1 THEN f.b ELSE f.a END
             WHERE f.a = ?1 OR f.b = ?1 ORDER BY u.name COLLATE NOCASE",
        )?;
        let rows = s.query_map([me], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get::<_, i64>(2)? == 1, r.get(3)?))
        })?;
        rows.collect()
    }

    pub fn are_friends(&self, a: i64, b: i64) -> R<bool> {
        Ok(self
            .0
            .query_row(
                "SELECT 1 FROM friends WHERE accepted = 1 AND ((a=?1 AND b=?2) OR (a=?2 AND b=?1))",
                params![a, b],
                |_| Ok(()),
            )
            .optional()?
            .is_some())
    }

    // ---- DMs ----
    pub fn open_dm(&self, a: i64, b: i64) -> R<i64> {
        let existing: Option<i64> = self
            .0
            .query_row(
                "SELECT d1.channel_id FROM dm_members d1 JOIN dm_members d2
                 ON d1.channel_id = d2.channel_id WHERE d1.user_id = ?1 AND d2.user_id = ?2",
                params![a, b],
                |r| r.get(0),
            )
            .optional()?;
        if let Some(id) = existing {
            return Ok(id);
        }
        self.0
            .execute("INSERT INTO channels(server_id, name) VALUES(NULL, 'dm')", [])?;
        let id = self.0.last_insert_rowid();
        for u in [a, b] {
            self.0.execute(
                "INSERT INTO dm_members(channel_id, user_id) VALUES(?1, ?2)",
                params![id, u],
            )?;
        }
        Ok(id)
    }

    pub fn dms_of(&self, me: i64) -> R<Vec<(i64, i64, String)>> {
        let mut s = self.0.prepare(
            "SELECT d1.channel_id, u.id, u.name FROM dm_members d1
             JOIN dm_members d2 ON d1.channel_id = d2.channel_id AND d2.user_id != ?1
             JOIN users u ON u.id = d2.user_id WHERE d1.user_id = ?1 ORDER BY d1.channel_id",
        )?;
        let rows = s.query_map([me], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?;
        rows.collect()
    }

    /// Todos que devem ser avisados quando `me` muda de estado.
    pub fn related_users(&self, me: i64) -> R<Vec<i64>> {
        let mut s = self.0.prepare(
            "SELECT CASE WHEN a = ?1 THEN b ELSE a END FROM friends WHERE a = ?1 OR b = ?1
             UNION SELECT m2.user_id FROM members m1 JOIN members m2 ON m1.server_id = m2.server_id
                   WHERE m1.user_id = ?1
             UNION SELECT d2.user_id FROM dm_members d1 JOIN dm_members d2
                   ON d1.channel_id = d2.channel_id WHERE d1.user_id = ?1",
        )?;
        let rows = s.query_map([me], |r| r.get(0))?;
        Ok(rows.collect::<R<Vec<i64>>>()?.into_iter().filter(|u| *u != me).collect())
    }
}
