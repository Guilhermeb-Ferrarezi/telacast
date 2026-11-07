-- Reações com emoji: uma linha por (mensagem, emoji, pessoa).
CREATE TABLE message_reactions (
    message_id BIGINT NOT NULL REFERENCES messages(id) ON DELETE CASCADE,
    emoji      TEXT   NOT NULL CHECK (char_length(emoji) BETWEEN 1 AND 16),
    user_id    BIGINT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (message_id, emoji, user_id)
);
CREATE INDEX message_reactions_message ON message_reactions (message_id, created_at);
