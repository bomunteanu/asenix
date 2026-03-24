-- Change frontier_diversity from FLOAT to JSONB so the full FrontierDiversityData
-- struct (entropy, max_entropy, normalized_entropy, cluster_sizes, k, atom_count)
-- is stored rather than just the scalar entropy value.
--
-- Existing FLOAT rows are migrated to {"entropy": <old_value>} so historical
-- data is preserved (normalized_entropy and cluster_sizes will be null for
-- pre-migration rows — callers should handle this gracefully).

ALTER TABLE metrics_snapshots
    ALTER COLUMN frontier_diversity TYPE JSONB
    USING jsonb_build_object('entropy', frontier_diversity);
