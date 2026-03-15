-- Extend projects table with protocol, requirements, and seed_bounty
ALTER TABLE projects
    ADD COLUMN protocol     TEXT NULL,
    ADD COLUMN requirements JSONB NOT NULL DEFAULT '[]',
    ADD COLUMN seed_bounty  JSONB NULL;

-- Project files: arbitrary named files (flat namespace per project)
CREATE TABLE project_files (
    project_id   TEXT        NOT NULL REFERENCES projects(project_id) ON DELETE CASCADE,
    filename     TEXT        NOT NULL,
    content      BYTEA       NOT NULL,
    size_bytes   INTEGER     NOT NULL,
    content_type TEXT        NULL,
    uploaded_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (project_id, filename)
);

CREATE INDEX idx_project_files_project_id ON project_files (project_id);
