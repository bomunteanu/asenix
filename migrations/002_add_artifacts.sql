-- Create artifacts table for content-addressed storage
CREATE TABLE IF NOT EXISTS artifacts (
    hash TEXT PRIMARY KEY, -- BLAKE3 hash of the stored content
    type TEXT NOT NULL CHECK (type IN ('blob', 'tree')),
    size_bytes BIGINT NOT NULL,
    media_type TEXT NULL, -- MIME type for blobs, null for trees
    uploaded_by TEXT NOT NULL REFERENCES agents(agent_id),
    uploaded_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Add artifact_tree_hash column to atoms table
ALTER TABLE atoms ADD COLUMN IF NOT EXISTS artifact_tree_hash TEXT NULL;

-- Add foreign key constraint from atoms.artifact_tree_hash to artifacts.hash
-- Note: This constraint is optional since artifacts can be uploaded before atoms reference them
-- We'll enforce this at the application level in publish_atoms

-- Create indexes for artifacts table
CREATE INDEX IF NOT EXISTS idx_artifacts_type ON artifacts (type);
CREATE INDEX IF NOT EXISTS idx_artifacts_uploaded_by ON artifacts (uploaded_by);
CREATE INDEX IF NOT EXISTS idx_artifacts_uploaded_at ON artifacts (uploaded_at);

-- Create index for atoms artifact_tree_hash for faster lookups
CREATE INDEX IF NOT EXISTS idx_atoms_artifact_tree_hash ON atoms (artifact_tree_hash) WHERE artifact_tree_hash IS NOT NULL;
