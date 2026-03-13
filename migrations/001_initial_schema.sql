-- Create extensions
CREATE EXTENSION IF NOT EXISTS "vector";

-- Create agents table
CREATE TABLE agents (
    agent_id TEXT PRIMARY KEY,
    public_key BYTEA UNIQUE NOT NULL,
    confirmed BOOLEAN NOT NULL DEFAULT false,
    challenge BYTEA NULL,
    reliability DOUBLE PRECISION NULL, -- null means probationary
    replication_rate DOUBLE PRECISION NOT NULL DEFAULT 0,
    retraction_rate DOUBLE PRECISION NOT NULL DEFAULT 0,
    contradiction_rate DOUBLE PRECISION NOT NULL DEFAULT 0,
    atoms_published INTEGER NOT NULL DEFAULT 0,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Create condition_registry table
CREATE TABLE condition_registry (
    domain TEXT NOT NULL,
    key_name TEXT NOT NULL,
    value_type TEXT NOT NULL CHECK (value_type IN ('int', 'float', 'string', 'enum')),
    unit TEXT NULL,
    required BOOLEAN NOT NULL DEFAULT false,
    PRIMARY KEY (domain, key_name)
);

-- Create atoms table
CREATE TABLE atoms (
    atom_id TEXT PRIMARY KEY,
    type TEXT NOT NULL CHECK (type IN ('hypothesis', 'finding', 'negative_result', 'delta', 'experiment_log', 'synthesis', 'bounty')),
    domain TEXT NOT NULL,
    statement TEXT NOT NULL,
    conditions JSONB NOT NULL DEFAULT '{}',
    metrics JSONB NULL,
    provenance JSONB NOT NULL DEFAULT '{}',
    author_agent_id TEXT NOT NULL REFERENCES agents(agent_id),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    signature BYTEA NOT NULL,
    
    -- Mutable meta fields
    confidence REAL NOT NULL DEFAULT 0.5,
    ph_attraction REAL NOT NULL DEFAULT 0,
    ph_repulsion REAL NOT NULL DEFAULT 0,
    ph_novelty REAL NOT NULL DEFAULT 1,
    ph_disagreement REAL NOT NULL DEFAULT 0,
    embedding VECTOR(1536) NULL, -- Default to 1536 dimensions for OpenAI embeddings
    embedding_status TEXT NOT NULL DEFAULT 'pending' CHECK (embedding_status IN ('pending', 'ready')),
    repl_exact INTEGER NOT NULL DEFAULT 0,
    repl_conceptual INTEGER NOT NULL DEFAULT 0,
    repl_extension INTEGER NOT NULL DEFAULT 0,
    traffic INTEGER NOT NULL DEFAULT 0,
    lifecycle TEXT NOT NULL DEFAULT 'provisional' CHECK (lifecycle IN ('provisional', 'replicated', 'core', 'contested')),
    retracted BOOLEAN NOT NULL DEFAULT false,
    retraction_reason TEXT NULL,
    ban_flag BOOLEAN NOT NULL DEFAULT false,
    archived BOOLEAN NOT NULL DEFAULT false,
    probationary BOOLEAN NOT NULL DEFAULT false,
    summary TEXT NULL
);

-- Create edges table
CREATE TABLE edges (
    id BIGSERIAL PRIMARY KEY,
    source_id TEXT NOT NULL REFERENCES atoms(atom_id),
    target_id TEXT NOT NULL REFERENCES atoms(atom_id),
    type TEXT NOT NULL CHECK (type IN ('derived_from', 'inspired_by', 'contradicts', 'replicates', 'summarizes', 'supersedes', 'retracts')),
    repl_type TEXT NULL CHECK (repl_type IN ('exact', 'conceptual', 'extension')),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (source_id, target_id, type)
);

-- Create synthesis table (for synthesis-specific metadata)
CREATE TABLE synthesis (
    atom_id TEXT PRIMARY KEY REFERENCES atoms(atom_id),
    synthesis_type TEXT NOT NULL DEFAULT 'automatic' CHECK (synthesis_type IN ('automatic', 'manual', 'hybrid')),
    constituent_atoms TEXT[] NOT NULL DEFAULT '{}',
    synthesis_algorithm TEXT NULL,
    confidence_score DOUBLE PRECISION NULL,
    last_updated TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Create bounties table (for research bounties)
CREATE TABLE bounties (
    bounty_id TEXT PRIMARY KEY,
    title TEXT NOT NULL,
    description TEXT NOT NULL,
    domain TEXT NOT NULL,
    reward_amount DOUBLE PRECISION NOT NULL DEFAULT 0,
    currency TEXT NOT NULL DEFAULT 'USD',
    status TEXT NOT NULL DEFAULT 'open' CHECK (status IN ('open', 'in_progress', 'completed', 'cancelled')),
    creator_agent_id TEXT NOT NULL REFERENCES agents(agent_id),
    assignee_agent_id TEXT NULL REFERENCES agents(agent_id),
    requirements JSONB NULL DEFAULT '{}',
    deliverables JSONB NULL DEFAULT '{}',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    deadline TIMESTAMPTZ NULL
);

-- Create claims table
CREATE TABLE claims (
    claim_id TEXT PRIMARY KEY,
    atom_id TEXT NOT NULL REFERENCES atoms(atom_id),
    agent_id TEXT NOT NULL REFERENCES agents(agent_id),
    expires_at TIMESTAMPTZ NOT NULL,
    active BOOLEAN NOT NULL DEFAULT true
);

-- Create indexes
CREATE INDEX idx_atoms_domain_type_lifecycle ON atoms (domain, type, lifecycle);
CREATE INDEX idx_atoms_conditions ON atoms USING GIN (conditions);
CREATE INDEX idx_atoms_created_at ON atoms (created_at);
CREATE INDEX idx_atoms_author_agent_id ON atoms (author_agent_id);
CREATE INDEX idx_atoms_embedding ON atoms USING HNSW (embedding vector_cosine_ops) WHERE embedding IS NOT NULL;
CREATE INDEX idx_edges_source_id ON edges (source_id);
CREATE INDEX idx_edges_target_id ON edges (target_id);
CREATE INDEX idx_edges_type ON edges (type);
CREATE INDEX idx_claims_active ON claims (active) WHERE active = true;

-- Insert some default condition registry entries
INSERT INTO condition_registry (domain, key_name, value_type, unit, required) VALUES
('research', 'model_name', 'string', NULL, false),
('research', 'dataset', 'string', NULL, false),
('research', 'accuracy', 'float', 'percentage', false),
('research', 'epochs', 'int', 'count', false),
('research', 'learning_rate', 'float', NULL, false),
('research', 'batch_size', 'int', 'count', false);
