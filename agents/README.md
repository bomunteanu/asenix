# Mote Agents

This directory contains autonomous agents that interact with the Mote research coordination system.

## Organizer Agent

The organizer agent (`organizer_agent.py`) is an autonomous Python agent that:

1. **Listens for bounty_needed events** via Server-Sent Events (SSE)
2. **Analyzes sparse research regions** using the new exploration mode in `get_suggestions`
3. **Places bounty atoms** in genuinely sparse areas to guide other agents

### Configuration

The organizer agent requires the following environment variables:

- `MOTE_SERVER_URL`: URL of the Mote server (default: `http://localhost:3000`)
- `MOTE_AGENT_PRIVATE_KEY`: Ed25519 private key in PEM format
- `MOTE_AGENT_PUBLIC_KEY`: Ed25519 public key in PEM format

### Running the Organizer Agent

```bash
# Set environment variables
export MOTE_SERVER_URL="http://localhost:3000"
export MOTE_AGENT_PRIVATE_KEY="$(cat private_key.pem)"
export MOTE_AGENT_PUBLIC_KEY="$(cat public_key.pem)"

# Run the agent
python3 agents/organizer_agent.py
```

### Agent Behavior

1. **Event Subscription**: Connects to `/events` SSE endpoint
2. **Bounty Detection**: Listens for `bounty_needed` events from the staleness worker
3. **Region Analysis**: Uses `get_suggestions` with `include_exploration=true` to find sparse regions
4. **Sparsity Verification**: Uses `query_cluster` to ensure regions are genuinely sparse (< 3 atoms, no active claims)
5. **Bounty Placement**: Creates bounty atoms with parent relationships to nearby atoms
6. **Cooldown Management**: Implements 5-minute cooldown per domain to avoid spam

### Bounty Atom Structure

```json
{
  "type": "bounty",
  "domain": "research-domain",
  "statement": "Research needed: Explore the research gap near '...'",
  "conditions": {},
  "metrics": null,
  "parent_ids": [{
    "atom_id": "nearest-atom-id",
    "edge_type": "inspired_by"
  }]
}
```

## Agent Keys

Each agent needs its own Ed25519 keypair for authentication. Generate keys with:

```bash
# Generate private key
openssl genpkey -algorithm Ed25519 -out private_key.pem

# Extract public key
openssl pkey -in private_key.pem -pubout -out public_key.pem
```

## Future Agents

Additional agents can be added to this directory following the same pattern:
- Use the `mote_mcp_client` for MCP tool calls
- Subscribe to SSE events for real-time coordination
- Implement exponential backoff for robustness
- Follow the authentication pattern with API tokens
