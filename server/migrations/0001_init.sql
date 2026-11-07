-- Esquema inicial do Telacast (PostgreSQL 18).
-- Todos os ids são BIGINT gerados pelo servidor (estilo snowflake), não SERIAL.

CREATE TABLE users (
    id           BIGINT PRIMARY KEY,
    username     TEXT NOT NULL CHECK (char_length(username) BETWEEN 3 AND 32),
    display_name TEXT NOT NULL CHECK (char_length(display_name) BETWEEN 1 AND 32),
    pass_hash    TEXT NOT NULL,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE UNIQUE INDEX users_username_lower ON users (lower(username));

-- Sessões: o token aparece uma vez para o cliente; no banco fica só o hash (SHA-256).
CREATE TABLE sessions (
    id           BIGINT PRIMARY KEY,
    user_id      BIGINT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    token_hash   BYTEA NOT NULL UNIQUE,
    device       TEXT,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    last_used_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    expires_at   TIMESTAMPTZ NOT NULL
);
CREATE INDEX sessions_user ON sessions (user_id);

-- Convites de cadastro da instância (cadastro fechado). Só o hash do código fica salvo.
CREATE TABLE instance_invites (
    id         BIGINT PRIMARY KEY,
    code_hash  BYTEA NOT NULL UNIQUE,
    label      TEXT,
    created_by BIGINT REFERENCES users(id) ON DELETE SET NULL,
    max_uses   INT NOT NULL DEFAULT 1 CHECK (max_uses >= 1),
    uses       INT NOT NULL DEFAULT 0,
    expires_at TIMESTAMPTZ,
    revoked_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CHECK (uses <= max_uses)
);

CREATE TABLE servers (
    id         BIGINT PRIMARY KEY,
    name       TEXT NOT NULL CHECK (char_length(name) BETWEEN 1 AND 50),
    owner_id   BIGINT NOT NULL REFERENCES users(id),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Convites de servidor: o código precisa ser mostrado na interface, então fica em claro.
CREATE TABLE server_invites (
    code       TEXT PRIMARY KEY,
    server_id  BIGINT NOT NULL REFERENCES servers(id) ON DELETE CASCADE,
    created_by BIGINT REFERENCES users(id) ON DELETE SET NULL,
    max_uses   INT CHECK (max_uses IS NULL OR max_uses >= 1),
    uses       INT NOT NULL DEFAULT 0,
    expires_at TIMESTAMPTZ,
    revoked_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX server_invites_server ON server_invites (server_id);

CREATE TABLE members (
    server_id BIGINT NOT NULL REFERENCES servers(id) ON DELETE CASCADE,
    user_id   BIGINT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    nickname  TEXT,
    joined_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (server_id, user_id)
);
CREATE INDEX members_user ON members (user_id);

-- kind: 0 texto, 1 voz, 2 anúncio, 3 palco, 4 categoria, 10 conversa privada (DM)
CREATE TABLE channels (
    id         BIGINT PRIMARY KEY,
    server_id  BIGINT REFERENCES servers(id) ON DELETE CASCADE,
    parent_id  BIGINT REFERENCES channels(id) ON DELETE SET NULL,
    kind       SMALLINT NOT NULL DEFAULT 0,
    name       TEXT NOT NULL CHECK (char_length(name) BETWEEN 1 AND 100),
    position   INT NOT NULL DEFAULT 0,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CHECK ((server_id IS NULL) = (kind = 10))
);
CREATE INDEX channels_server ON channels (server_id, position, id);

-- Participantes de DMs (e, no futuro, de DMs em grupo).
CREATE TABLE channel_members (
    channel_id BIGINT NOT NULL REFERENCES channels(id) ON DELETE CASCADE,
    user_id    BIGINT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    PRIMARY KEY (channel_id, user_id)
);
CREATE INDEX channel_members_user ON channel_members (user_id);

CREATE TABLE messages (
    id         BIGINT PRIMARY KEY,
    channel_id BIGINT NOT NULL REFERENCES channels(id) ON DELETE CASCADE,
    author_id  BIGINT NOT NULL REFERENCES users(id),
    content    TEXT NOT NULL CHECK (char_length(content) BETWEEN 1 AND 4000),
    edited_at  TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX messages_channel_id ON messages (channel_id, id DESC);

-- requester pediu; accepted = false é pedido pendente, true é amizade.
CREATE TABLE friendships (
    requester_id BIGINT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    target_id    BIGINT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    accepted     BOOLEAN NOT NULL DEFAULT false,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (requester_id, target_id),
    CHECK (requester_id <> target_id)
);
CREATE INDEX friendships_target ON friendships (target_id);

CREATE TABLE blocks (
    user_id    BIGINT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    blocked_id BIGINT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (user_id, blocked_id),
    CHECK (user_id <> blocked_id)
);

-- Para "não lidos" e contagem de menções.
CREATE TABLE read_state (
    user_id       BIGINT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    channel_id    BIGINT NOT NULL REFERENCES channels(id) ON DELETE CASCADE,
    last_read_id  BIGINT NOT NULL DEFAULT 0,
    mention_count INT NOT NULL DEFAULT 0,
    PRIMARY KEY (user_id, channel_id)
);
