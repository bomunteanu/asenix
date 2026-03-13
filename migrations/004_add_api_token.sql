-- Add API token support for simple agent authentication (no Ed25519 keypair required).
-- Agents registered via register_agent_simple use this token instead of per-request signatures.
ALTER TABLE agents ADD COLUMN api_token TEXT UNIQUE NULL;

CREATE INDEX idx_agents_api_token ON agents (api_token) WHERE api_token IS NOT NULL;
