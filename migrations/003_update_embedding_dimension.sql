-- Update embedding column from VECTOR(1536) to VECTOR(384) for local model support.
-- All existing (fake/hash-based) embeddings are invalidated and will be re-generated
-- by the embedding worker using the configured provider.

DROP INDEX IF EXISTS idx_atoms_embedding;

ALTER TABLE atoms DROP COLUMN IF EXISTS embedding;
ALTER TABLE atoms ADD COLUMN embedding VECTOR(384) NULL;

UPDATE atoms SET embedding_status = 'pending';

CREATE INDEX idx_atoms_embedding ON atoms USING HNSW (embedding vector_cosine_ops) WHERE embedding IS NOT NULL;
