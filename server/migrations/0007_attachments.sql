-- Uma mensagem pode ter só anexos (texto vazio); o limite de 4000 caracteres continua.
ALTER TABLE messages DROP CONSTRAINT IF EXISTS messages_content_check;
ALTER TABLE messages ADD CONSTRAINT messages_content_len CHECK (char_length(content) <= 4000);

-- Anexos: o arquivo fica em disco, endereçado pelo hash do conteúdo; aqui ficam os metadados.
-- `message_id` é NULL entre o envio do arquivo e o envio da mensagem.
CREATE TABLE attachments (
    id          BIGINT PRIMARY KEY,
    uploader_id BIGINT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    channel_id  BIGINT NOT NULL REFERENCES channels(id) ON DELETE CASCADE,
    message_id  BIGINT REFERENCES messages(id) ON DELETE CASCADE,
    name        TEXT   NOT NULL CHECK (char_length(name) BETWEEN 1 AND 200),
    mime        TEXT   NOT NULL,
    size        BIGINT NOT NULL,
    width       INT,
    height      INT,
    hash        TEXT   NOT NULL,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX attachments_message ON attachments (message_id);
CREATE INDEX attachments_pending ON attachments (uploader_id) WHERE message_id IS NULL;
