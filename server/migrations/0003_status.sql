-- Status escolhido pelo usuário: 0 online, 1 ausente, 2 não perturbe, 3 invisível.
ALTER TABLE users ADD COLUMN status SMALLINT NOT NULL DEFAULT 0 CHECK (status BETWEEN 0 AND 3);
