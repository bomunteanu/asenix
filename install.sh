#!/usr/bin/env bash
# install.sh — builds and installs the asenix CLI + pre-installs bundled domains
set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

# ── Colour helpers ────────────────────────────────────────────────────────────
GREEN='\033[0;32m'; CYAN='\033[0;36m'; YELLOW='\033[1;33m'; RED='\033[0;31m'; NC='\033[0m'
ok()   { echo -e "${GREEN}✓${NC} $*"; }
info() { echo -e "${CYAN}▶${NC} $*"; }
warn() { echo -e "${YELLOW}⚠${NC} $*"; }
fail() { echo -e "${RED}✗${NC} $*"; exit 1; }

echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "  Asenix Installer"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo ""

# ── Resolve OS-native directories ─────────────────────────────────────────────
case "$OSTYPE" in
  darwin*)
    DEFAULT_DATA_DIR="$HOME/Library/Application Support/asenix"
    DEFAULT_BIN_DIR="/usr/local/bin"
    ;;
  linux*)
    DEFAULT_DATA_DIR="${XDG_DATA_HOME:-$HOME/.local/share}/asenix"
    DEFAULT_BIN_DIR="$HOME/.local/bin"
    ;;
  *)
    DEFAULT_DATA_DIR="$HOME/.asenix"
    DEFAULT_BIN_DIR="$HOME/.local/bin"
    ;;
esac

DATA_DIR="${ASENIX_DATA_DIR:-$DEFAULT_DATA_DIR}"
BIN_DIR="${ASENIX_BIN_DIR:-$DEFAULT_BIN_DIR}"

info "Binary  → $BIN_DIR/asenix"
info "Data    → $DATA_DIR"
echo ""

# ── Check prerequisites ───────────────────────────────────────────────────────
command -v cargo >/dev/null 2>&1 || fail "Rust not found. Install from https://rustup.rs"
ok "Rust found ($(rustc --version | cut -d' ' -f2))"

# ── Build ─────────────────────────────────────────────────────────────────────
info "Building release binary..."
cd "$SCRIPT_DIR"
# Suppress the noise but show errors
if ! cargo build --release --bin asenix 2>&1 | grep -E "^error" | head -20; then
  cargo build --release --bin asenix 2>/dev/null
fi
ok "Build complete"

# ── Install binary ────────────────────────────────────────────────────────────
mkdir -p "$BIN_DIR"
cp "$SCRIPT_DIR/target/release/asenix" "$BIN_DIR/asenix"
chmod +x "$BIN_DIR/asenix"
ok "Installed asenix → $BIN_DIR/asenix"

# ── Create data directory structure ───────────────────────────────────────────
mkdir -p "$DATA_DIR/domains"
mkdir -p "$DATA_DIR/logs"
ok "Data directory ready: $DATA_DIR"

# ── Pre-install bundled domain packs ─────────────────────────────────────────
echo ""
info "Installing bundled domain packs..."

install_domain() {
  local name="$1"
  local dest="$DATA_DIR/domains/$name"
  shift

  mkdir -p "$dest"
  for src in "$@"; do
    if [[ -f "$src" ]]; then
      cp "$src" "$dest/"
    elif [[ -d "$src" ]]; then
      cp -r "$src" "$dest/"
    fi
  done
  ok "Domain '$name' → $dest"
}

# cifar10_resnet — assembled from demo/
DEMO="$SCRIPT_DIR/demo"
if [[ -d "$DEMO" ]]; then
  CIFAR_DEST="$DATA_DIR/domains/cifar10_resnet"
  mkdir -p "$CIFAR_DEST/files"
  cp "$DEMO/domain.toml"         "$CIFAR_DEST/"
  cp "$DEMO/CLAUDE.md"           "$CIFAR_DEST/"
  cp "$DEMO/bounty.json"         "$CIFAR_DEST/"
  cp "$DEMO/train.py"            "$CIFAR_DEST/files/"
  cp "$DEMO/synthesis_agent.py"  "$CIFAR_DEST/files/" 2>/dev/null || true
  # Write requirements.txt for this domain
  cat > "$CIFAR_DEST/requirements.txt" <<'EOF'
torch
torchvision
matplotlib
numpy
networkx
requests
EOF
  ok "Domain 'cifar10_resnet' → $CIFAR_DEST"
fi

# Any other packs under domains/ in the repo
if [[ -d "$SCRIPT_DIR/domains" ]]; then
  for pack_dir in "$SCRIPT_DIR/domains/"/; do
    [[ -f "$pack_dir/domain.toml" ]] || continue
    name=$(grep '^name' "$pack_dir/domain.toml" | head -1 | sed 's/.*= *"\(.*\)"/\1/')
    [[ -z "$name" ]] && continue
    dest="$DATA_DIR/domains/$name"
    rm -rf "$dest"
    cp -r "$pack_dir" "$dest"
    ok "Domain '$name' → $dest"
  done
fi

# ── PATH reminder ─────────────────────────────────────────────────────────────
echo ""
if ! echo "$PATH" | tr ':' '\n' | grep -qx "$BIN_DIR"; then
  warn "$BIN_DIR is not in your PATH"
  if [[ "$OSTYPE" == "darwin"* ]]; then
    echo "  Add to ~/.zshrc (or ~/.bash_profile):"
  else
    echo "  Add to ~/.bashrc (or ~/.zshrc):"
  fi
  echo ""
  echo "    export PATH=\"$BIN_DIR:\$PATH\""
  echo ""
  echo "  Then reload: source ~/.zshrc"
  echo ""
fi

echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "  Installation complete. What's next:"
echo ""
echo "  1. Start the stack"
echo "       asenix up"
echo ""
echo "  2. Launch research agents"
echo "       asenix agent run --domain cifar10_resnet --n 3"
echo ""
echo "  3. Watch what they're doing"
echo "       asenix logs"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
