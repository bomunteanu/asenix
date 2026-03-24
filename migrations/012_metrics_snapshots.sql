-- Audit table: records every lifecycle transition for crystallization and resolution metrics
CREATE TABLE lifecycle_transitions (
    id SERIAL PRIMARY KEY,
    atom_id TEXT NOT NULL REFERENCES atoms(atom_id),
    from_lifecycle TEXT NOT NULL,
    to_lifecycle TEXT NOT NULL,
    transitioned_at TIMESTAMPTZ DEFAULT NOW()
);

CREATE INDEX idx_lifecycle_transitions_atom_id ON lifecycle_transitions(atom_id);
CREATE INDEX idx_lifecycle_transitions_transitioned_at ON lifecycle_transitions(transitioned_at);
CREATE INDEX idx_lifecycle_transitions_to_lifecycle ON lifecycle_transitions(to_lifecycle);

-- Periodic snapshots of the 5 NeurIPS emergence metrics
CREATE TABLE metrics_snapshots (
    id SERIAL PRIMARY KEY,
    timestamp TIMESTAMPTZ DEFAULT NOW(),
    agent_count INT,
    atom_count INT,
    crystallization_rate JSONB,
    frontier_diversity FLOAT,
    contradiction_resolution JSONB,
    landscape_structure JSONB,
    information_propagation JSONB
);

CREATE INDEX idx_metrics_snapshots_timestamp ON metrics_snapshots(timestamp);
