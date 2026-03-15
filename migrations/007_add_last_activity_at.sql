-- Add last_activity_at to track the most recent significant event on an atom.
-- Decay uses this instead of created_at so that atoms receiving new edges
-- (replications, contradictions, derivations) reset their decay clock.
ALTER TABLE atoms ADD COLUMN IF NOT EXISTS last_activity_at TIMESTAMPTZ NOT NULL DEFAULT NOW();

-- Back-fill existing atoms: start their clock at creation time.
UPDATE atoms SET last_activity_at = created_at WHERE last_activity_at > created_at OR last_activity_at = NOW();
