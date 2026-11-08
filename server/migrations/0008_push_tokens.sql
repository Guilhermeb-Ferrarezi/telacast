-- Tokens de notificação push (FCM). Um token pertence a um único usuário: se outra conta registra o mesmo
-- token (o aparelho trocou de dono), ele muda de dono. `last_seen` é renovado a cada registro e decide
-- quem sai quando a conta passa do limite de tokens.
CREATE TABLE push_tokens (
    user_id    BIGINT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    token      TEXT   NOT NULL CHECK (char_length(token) BETWEEN 1 AND 4096),
    platform   TEXT   NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    last_seen  TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (token)
);
CREATE INDEX push_tokens_user ON push_tokens (user_id);
