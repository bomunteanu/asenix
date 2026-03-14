#!/usr/bin/env bash
# run_agent.sh — launch a Claude Code agent in the demo directory
#
# Usage:
#   ./run_agent.sh        # uses .agent_config (agent 1)
#   ./run_agent.sh 2      # uses .agent2_config
#   ./run_agent.sh 3      # uses .agent3_config

set -e
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR"

SERVER_URL="${ASENIX_URL:-http://localhost:3000}"

# ── Resolve config file ───────────────────────────────────────────────────────
AGENT_NUM="${1:-1}"
if [ "$AGENT_NUM" = "1" ]; then
    CONFIG_FILE=".agent_config"
else
    CONFIG_FILE=".agent${AGENT_NUM}_config"
fi

TIMESTAMP=$(date +%Y%m%d_%H%M%S)
LOG_FILE="logs/agent${AGENT_NUM}_${TIMESTAMP}.log"

echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo " Asenix Agent ${AGENT_NUM} Launch"
echo " Config: $CONFIG_FILE"
echo " Log:    $LOG_FILE"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"

# ── Pre-flight checks ─────────────────────────────────────────────────────────
if [ ! -f "$CONFIG_FILE" ]; then
    echo "✗ $CONFIG_FILE not found. Run ./setup.sh to register a new agent."
    exit 1
fi

source "$CONFIG_FILE"
if [ -z "$AGENT_ID" ] || [ -z "$API_TOKEN" ]; then
    echo "✗ AGENT_ID or API_TOKEN missing in $CONFIG_FILE. Re-run ./setup.sh."
    exit 1
fi

echo "✓ Agent credentials loaded:"
echo "  agent_id = $AGENT_ID"
echo "  server   = $SERVER_URL"

# Check server is up
curl -sf "$SERVER_URL/health" > /dev/null || {
    echo "✗ Asenix server not reachable at $SERVER_URL"
    echo "  Start it: docker-compose up  (from project root)"
    exit 1
}
echo "✓ Server healthy"

# Check claude is installed
command -v claude > /dev/null || {
    echo "✗ 'claude' CLI not found. Install Claude Code first."
    exit 1
}
echo "✓ Claude Code found: $(claude --version 2>/dev/null || echo 'version unknown')"

mkdir -p logs

# ── Launch ────────────────────────────────────────────────────────────────────
echo ""
echo "▶ Launching agent (log → $LOG_FILE) ..."
echo "  Press Ctrl+C to stop."
echo ""

claude \
    --dangerously-skip-permissions \
    -p "You are a research agent in the demo/ directory. Follow CLAUDE.md exactly. Your credentials are: AGENT_ID=$AGENT_ID and API_TOKEN=$API_TOKEN. Start by reading CLAUDE.md, then begin the CIFAR-10 research loop. Run at least 4 full training iterations before stopping, each time choosing a new hypothesis based on what you've already published to the knowledge graph." \
    2>&1 | tee "$LOG_FILE"

EXIT_CODE=${PIPESTATUS[0]}
echo ""
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo " Agent exited with code $EXIT_CODE"
echo " Full log: $LOG_FILE"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
