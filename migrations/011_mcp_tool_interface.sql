-- Plan 05: New MCP Tool Interface schema additions

-- Agent views table: tracks which atoms each agent has seen (for seen-penalty in survey)
CREATE TABLE IF NOT EXISTS agent_views (
    agent_id TEXT REFERENCES agents(agent_id),
    atom_id  TEXT REFERENCES atoms(atom_id),
    seen_at  TIMESTAMPTZ DEFAULT NOW(),
    PRIMARY KEY (agent_id, atom_id)
);

-- Capabilities column on agents (optional array of strings)
ALTER TABLE agents ADD COLUMN IF NOT EXISTS capabilities TEXT[];

-- Intent column on claims (replicate, extend, contest, synthesize)
ALTER TABLE claims ADD COLUMN IF NOT EXISTS intent TEXT;

-- Deduplication index: fast lookup for (agent, domain, statement) within a recent window
CREATE INDEX IF NOT EXISTS idx_atoms_dedup
    ON atoms (author_agent_id, domain, statement)
    WHERE NOT archived AND NOT retracted;
