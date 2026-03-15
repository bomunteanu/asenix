-- Projects table: top-level namespace for atoms, bounties, and conditions
CREATE TABLE projects (
    project_id  TEXT PRIMARY KEY,
    name        TEXT NOT NULL,
    slug        TEXT NOT NULL UNIQUE,
    description TEXT NULL,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Add project_id to atoms (nullable for backward compatibility)
ALTER TABLE atoms
    ADD COLUMN project_id TEXT NULL REFERENCES projects(project_id);

-- Indexes
CREATE INDEX idx_atoms_project_id ON atoms (project_id) WHERE project_id IS NOT NULL;

-- Seed the "CIFAR-10 ResNet Search" project
INSERT INTO projects (project_id, name, slug, description, created_at)
VALUES (
    'proj_cifar10_resnet',
    'CIFAR-10 ResNet Search',
    'cifar10-resnet-search',
    'Automated hyperparameter and architecture search for ResNet on CIFAR-10',
    NOW()
);

-- Assign all existing cifar10_resnet-domain atoms to this project
UPDATE atoms
    SET project_id = 'proj_cifar10_resnet'
    WHERE domain = 'cifar10_resnet';
