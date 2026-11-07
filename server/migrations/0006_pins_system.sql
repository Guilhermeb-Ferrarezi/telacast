-- Tipo da mensagem: 0 normal, 2 alguém entrou no servidor, 3 alguém fixou uma mensagem
-- (nesta, `content` guarda o id da mensagem fixada).
ALTER TABLE messages ADD COLUMN kind SMALLINT NOT NULL DEFAULT 0;
ALTER TABLE messages ADD COLUMN pinned_at TIMESTAMPTZ;
CREATE INDEX messages_pinned ON messages (channel_id, pinned_at DESC) WHERE pinned_at IS NOT NULL;
