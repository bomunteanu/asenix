-- Fix lifecycle constraint to include 'resolved' and 'retracted' states
-- added by the lifecycle state machine (Plan 04) but missing from the initial schema.

ALTER TABLE atoms DROP CONSTRAINT IF EXISTS atoms_lifecycle_check;
ALTER TABLE atoms ADD CONSTRAINT atoms_lifecycle_check
    CHECK (lifecycle IN ('provisional', 'replicated', 'core', 'contested', 'resolved', 'retracted'));
