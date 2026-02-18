#!/bin/bash
set -e

RTK_BIN="./target/debug/rtk"
GREEN='\033[0;32m'
RED='\033[0;31m'
NC='\033[0m'
PASS=0
FAIL=0

HOME_SAV=$HOME
TEST_HOME=$(mktemp -d)
export HOME=$TEST_HOME
trap 'export HOME=$HOME_SAV; rm -rf "$TEST_HOME"' EXIT

check() {
  local label=$1
  local file=$2
  if [ -f "$file" ] || [ -d "$file" ]; then
    echo -e "${GREEN}✅ $label${NC}"
    PASS=$((PASS + 1))
  else
    echo -e "${RED}❌ $label — not found: $file${NC}"
    FAIL=$((FAIL + 1))
  fi
}

# Ensure binary exists
if [ ! -f "$RTK_BIN" ]; then
    echo "Building rtk..."
    cargo build
fi

echo "--- Cas 1: Claude only ---"
mkdir -p "$HOME/.claude"
$RTK_BIN init -g --auto-patch > /dev/null
check "Claude: settings.json" "$HOME/.claude/settings.json"
check "Claude: hook"          "$HOME/.claude/hooks/rtk-rewrite.sh"
rm -rf "$HOME/.claude" "$HOME/.gemini"

echo "--- Cas 2: Gemini only ---"
mkdir -p "$HOME/.gemini"
$RTK_BIN init -g --auto-patch > /dev/null
check "Gemini: settings.json" "$HOME/.gemini/settings.json"
check "Gemini: hook"          "$HOME/.gemini/hooks/rtk-rewrite.sh"
check "Gemini: GEMINI.md"     "$HOME/.gemini/GEMINI.md"
rm -rf "$HOME/.claude" "$HOME/.gemini"

echo "--- Cas 3: Both ---"
mkdir -p "$HOME/.claude" "$HOME/.gemini"
$RTK_BIN init -g --auto-patch > /dev/null
check "Both: Claude settings.json" "$HOME/.claude/settings.json"
check "Both: Gemini settings.json" "$HOME/.gemini/settings.json"
rm -rf "$HOME/.claude" "$HOME/.gemini"

echo "--- Cas 4: No CLI ---"
output=$($RTK_BIN init -g 2>&1 || true)
if echo "$output" | grep -q "No CLI detected"; then
  echo -e "${GREEN}✅ No CLI: message correct${NC}"
  PASS=$((PASS + 1))
else
  echo -e "${RED}❌ No CLI: message manquant${NC}"
  FAIL=$((FAIL + 1))
fi

echo ""
echo "Results: $PASS passed, $FAIL failed"
[ $FAIL -eq 0 ] && echo -e "${GREEN}✨ ALL PASSED ✨${NC}" || exit 1
