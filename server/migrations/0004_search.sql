-- Busca de texto completo nas mensagens (português: radicais e palavras vazias).
CREATE INDEX messages_fts ON messages USING GIN (to_tsvector('portuguese', content));
