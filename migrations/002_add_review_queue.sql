-- Add review queue functionality

-- Add review_status column to atoms table
ALTER TABLE atoms ADD COLUMN review_status TEXT NOT NULL DEFAULT 'pending' CHECK (review_status IN ('pending', 'approved', 'rejected', 'auto_approved'));

-- Create reviews table for persistent review state
CREATE TABLE reviews (
    review_id TEXT PRIMARY KEY,
    atom_id TEXT NOT NULL REFERENCES atoms(atom_id),
    reviewer_agent_id TEXT NOT NULL REFERENCES agents(agent_id),
    decision TEXT NOT NULL CHECK (decision IN ('approve', 'reject', 'auto_approve')),
    reason TEXT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(atom_id, reviewer_agent_id) -- One review per atom per reviewer
);

-- Create indexes for review queue performance
CREATE INDEX idx_atoms_review_status ON atoms (review_status) WHERE review_status = 'pending';
CREATE INDEX idx_reviews_atom_id ON reviews (atom_id);
CREATE INDEX idx_reviews_decision ON reviews (decision);
CREATE INDEX idx_reviews_created_at ON reviews (created_at);

-- Add trigger to auto-approve atoms from high-reliability agents
CREATE OR REPLACE FUNCTION auto_approve_high_reliability_atoms()
RETURNS TRIGGER AS $$
BEGIN
    -- Auto-approve atoms from agents with reliability >= 0.8
    IF NEW.review_status = 'pending' THEN
        UPDATE atoms 
        SET review_status = 'auto_approved' 
        WHERE atom_id = NEW.atom_id 
        AND author_agent_id IN (
            SELECT agent_id FROM agents 
            WHERE reliability >= 0.8 
            AND atoms_published >= 5 -- Minimum publication threshold
        );
    END IF;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER trigger_auto_approve_high_reliability
    AFTER INSERT ON atoms
    FOR EACH ROW
    EXECUTE FUNCTION auto_approve_high_reliability_atoms();
