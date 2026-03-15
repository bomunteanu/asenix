#!/usr/bin/env bash
# setup.sh — one-time setup for the Asenix CIFAR-10 agent demo
# Run this before run_agent.sh

set -e
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR"

SERVER_URL="${ASENIX_URL:-http://localhost:3000}"

echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo " Asenix Demo Setup — CIFAR-10 Architecture Search"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"

# ── 1. Check server health ────────────────────────────────────────────────────
echo ""
echo "▶ Checking Asenix server at $SERVER_URL ..."
HEALTH=$(curl -sf "$SERVER_URL/health" 2>&1) || {
    echo "✗ Server not reachable. Start it first:"
    echo "    cd .. && docker-compose up   (or: cargo run -- --config config.toml)"
    exit 1
}
echo "✓ Server healthy"
echo "  $HEALTH" | python3 -m json.tool 2>/dev/null || echo "  $HEALTH"

# ── 2. Install Python deps ────────────────────────────────────────────────────
echo ""
echo "▶ Installing Python dependencies ..."
pip install torch torchvision requests matplotlib numpy networkx --quiet && echo "✓ Python deps installed" || {
    echo "✗ pip install failed — check your Python environment"
    exit 1
}

# ── 3. Register agent ─────────────────────────────────────────────────────────
echo ""
echo "▶ Registering agent with Asenix ..."
AGENT_NAME="cifar-agent-$(date +%s)"
RESPONSE=$(curl -sf -X POST "$SERVER_URL/rpc" \
    -H "Content-Type: application/json" \
    -d "{\"jsonrpc\":\"2.0\",\"method\":\"register_agent_simple\",\"params\":{\"agent_name\":\"$AGENT_NAME\"},\"id\":1}") || {
    echo "✗ Registration request failed"
    exit 1
}

echo "  Raw response: $RESPONSE"

AGENT_ID=$(python3 -c "import json,sys; d=json.loads('$RESPONSE'); print(d['result']['agent_id'])" 2>/dev/null) || {
    echo "✗ Failed to parse agent_id from response"
    echo "  Response was: $RESPONSE"
    exit 1
}
API_TOKEN=$(python3 -c "import json,sys; d=json.loads('$RESPONSE'); print(d['result']['api_token'])" 2>/dev/null) || {
    echo "✗ Failed to parse api_token from response"
    exit 1
}

echo "✓ Registered agent:"
echo "  agent_id  = $AGENT_ID"
echo "  api_token = $API_TOKEN"

# ── 4. Write .agent_config (auto-number if slot already taken) ────────────────
CONFIG_FILE=".agent_config"
N=2
while [ -f "$CONFIG_FILE" ]; do
    CONFIG_FILE=".agent${N}_config"
    N=$((N + 1))
done

cat > "$CONFIG_FILE" <<EOF
AGENT_ID=$AGENT_ID
API_TOKEN=$API_TOKEN
SERVER_URL=$SERVER_URL
AGENT_NAME=$AGENT_NAME
EOF
echo ""
echo "✓ Credentials saved to $CONFIG_FILE"

# ── 5. Post seed bounty ───────────────────────────────────────────────────────
echo ""
echo "▶ Posting seed bounty to bootstrap exploration ..."
BOUNTY_PAYLOAD=$(python3 -c "
import json, sys
payload = {
    'jsonrpc': '2.0',
    'method': 'publish_atoms',
    'id': 2,
    'params': {
        'agent_id': sys.argv[1],
        'api_token': sys.argv[2],
        'atoms': [{
            'atom_type': 'bounty',
            'domain': 'cifar10_resnet',
            'statement': 'Maximise CIFAR-10 val_accuracy using a 3-stage residual network (train.py). Baseline: SGD+cosine+standard_aug, num_blocks=[2,2,2], base_channels=32, 20 epochs achieves ~0.83-0.86. Target: >0.92. Key axes to explore: (1) strong augmentation + label smoothing, (2) OneCycleLR with higher peak LR, (3) wider/deeper networks, (4) AdamW vs SGD.',
            'conditions': {
                'num_blocks': None,
                'base_channels': None,
                'optimizer': None,
                'scheduler': None,
                'augmentation': None,
                'label_smoothing': None,
                'dropout': 0.1,
                'batch_size': 128,
                'weight_decay': 0.0005
            },
            'metrics': [
                {'name': 'val_accuracy',  'direction': 'maximize'},
                {'name': 'val_loss',      'direction': 'minimize'},
                {'name': 'train_time_s',  'direction': 'minimize', 'unit': 'seconds'}
            ],
            'provenance': {'method_description': 'Human-authored seed bounty for CIFAR-10 architecture search'}
        }]
    }
}
print(json.dumps(payload))
" "$AGENT_ID" "$API_TOKEN")
curl -sf -X POST "$SERVER_URL/rpc" \
    -H "Content-Type: application/json" \
    -d "$BOUNTY_PAYLOAD" > /dev/null && echo "✓ Seed bounty posted" || echo "⚠ Bounty post failed (server may be fresh — continuing)"

# ── 6. Create result dirs ─────────────────────────────────────────────────────
mkdir -p results logs
echo "✓ Created results/ and logs/ directories"

# ── Done ──────────────────────────────────────────────────────────────────────
echo ""
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo " Setup complete. Next steps:"
echo "   1. Start the agent:        ./run_agent.sh"
echo "   2. Watch metrics:          python visualize.py --domain cifar10_resnet"
echo "   3. Watch knowledge graph:  python visualize_graph.py --domain cifar10_resnet"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
