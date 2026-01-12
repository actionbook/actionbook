-- Add vector indexes for fast similarity search on chunks.embedding
-- HNSW (Hierarchical Navigable Small World) index for approximate nearest neighbor search

-- Create HNSW index on chunks.embedding for cosine distance
-- m=16: number of connections per layer (default 16, higher = better recall but slower build)
-- ef_construction=64: size of dynamic candidate list (default 64, higher = better index quality)
CREATE INDEX IF NOT EXISTS chunks_embedding_hnsw_idx
ON chunks
USING hnsw (embedding vector_cosine_ops)
WITH (m = 16, ef_construction = 64);

-- Add index on source_version_id for faster joins in search queries
CREATE INDEX IF NOT EXISTS chunks_source_version_id_idx
ON chunks (source_version_id)
WHERE source_version_id IS NOT NULL;

-- Add composite index for common search filter pattern
-- This helps with queries that filter by source_version_id and check for embedding
CREATE INDEX IF NOT EXISTS chunks_version_embedding_idx
ON chunks (source_version_id)
WHERE embedding IS NOT NULL;
