#!/usr/bin/env bash
# reset.sh — wipe the Asenix DB and local demo state for a clean run
# WARNING: This deletes ALL atoms, agents, edges, and embeddings.

set -e
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR"

DB_URL="${DATABASE_URL:-postgres://asenix:asenix_password@localhost:5432/asenix}"

echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo " Asenix Demo Reset"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo ""
echo "⚠  This will permanently delete:"
echo "     - All atoms, agents, edges, pheromone data"
echo "     - Local .agent_config"
echo "     - results/ and logs/ directories"
echo ""
read -p "Are you sure? Type 'yes' to confirm: " CONFIRM
if [[ "$CONFIRM" != "yes" ]]; then
    echo "Aborted."
    exit 0
fi

# ── Truncate DB tables ────────────────────────────────────────────────────────
echo ""
echo "▶ Truncating database tables ..."
psql "$DB_URL" <<'SQL'
TRUNCATE
    atom_artifacts,
    edges,
    atoms,
    research_claims,
    agents
RESTART IDENTITY CASCADE;
SQL
echo "✓ Database cleared"

# ── Wipe local state ──────────────────────────────────────────────────────────
echo ""
echo "▶ Clearing local state ..."
rm -f .agent_config
rm -rf results logs __pycache__
mkdir -p results logs
echo "✓ Local state cleared"

echo ""
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo " Reset complete."
echo " Restart the Asenix server to clear in-memory state:"
echo "   cargo run -- --config ../config.toml"
echo " Then re-run setup:"
echo "   ./setup.sh"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
