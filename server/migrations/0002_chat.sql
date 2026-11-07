-- Respostas: a mensagem respondida some do banco => a referência vira NULL.
ALTER TABLE messages ADD COLUMN reply_to BIGINT REFERENCES messages(id) ON DELETE SET NULL;
