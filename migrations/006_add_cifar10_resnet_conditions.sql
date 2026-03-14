-- Condition registry for the cifar10_resnet domain.
-- num_blocks and base_channels are required: two atoms are only "comparable"
-- when both keys are present and match (enables contradiction/replication detection).
-- All other keys are optional: recorded for reproducibility.
INSERT INTO condition_registry (domain, key_name, value_type, unit, required) VALUES
    ('cifar10_resnet', 'num_blocks',      'string', NULL,    true),
    ('cifar10_resnet', 'base_channels',   'int',    'count', true),
    ('cifar10_resnet', 'optimizer',       'string', NULL,    false),
    ('cifar10_resnet', 'scheduler',       'string', NULL,    false),
    ('cifar10_resnet', 'augmentation',    'string', NULL,    false),
    ('cifar10_resnet', 'learning_rate',   'float',  NULL,    false),
    ('cifar10_resnet', 'label_smoothing', 'float',  NULL,    false),
    ('cifar10_resnet', 'dropout',         'float',  NULL,    false),
    ('cifar10_resnet', 'weight_decay',    'float',  NULL,    false),
    ('cifar10_resnet', 'batch_size',      'int',    'count', false)
ON CONFLICT (domain, key_name) DO NOTHING;
