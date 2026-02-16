#!/bin/bash
# Test Gemini Initialization for RTK

set -e

# Setup temporary home to avoid polluting real home
TEST_HOME=$(mktemp -d)
export HOME=$TEST_HOME
echo "Using temporary HOME: $HOME"

# Define colors
GREEN='\033[0;32m'
RED='\033[0;31m'
NC='\033[0m'

# Paths
RTK_BIN="./target/debug/rtk"
GEMINI_DIR="$HOME/.gemini"
GEMINI_MD="$GEMINI_DIR/GEMINI.md"
RTK_MD="$GEMINI_DIR/RTK.md"
HOOK_PATH="$GEMINI_DIR/hooks/rtk-rewrite.sh"

echo "1. Testing Fresh Gemini Installation..."
$RTK_BIN init -g --gemini > /dev/null

if [ -f "$GEMINI_MD" ] && [ -f "$RTK_MD" ] && [ -x "$HOOK_PATH" ]; then
    echo -e "${GREEN}✅ Fresh installation files created successfully${NC}"
else
    echo -e "${RED}❌ Fresh installation failed${NC}"
    exit 1
fi

if grep -q "rtk git" "$GEMINI_MD"; then
    echo -e "${GREEN}✅ GEMINI.md contains correct instructions${NC}"
else
    echo -e "${RED}❌ GEMINI.md content is wrong${NC}"
    exit 1
fi

echo "2. Testing Upsert (Preserve User Content)..."
# Create a file with user content and an old RTK block
cat > "$GEMINI_MD" <<EOF
# User Section
My custom notes here.

<!-- rtk-instructions v2 -->
OLD RTK CONTENT
<!-- /rtk-instructions -->

End of user file.
EOF

$RTK_BIN init -g --gemini > /dev/null

if grep -q "My custom notes here." "$GEMINI_MD" && grep -q "End of user file." "$GEMINI_MD"; then
    echo -e "${GREEN}✅ User content preserved during update${NC}"
else
    echo -e "${RED}❌ User content LOST during update${NC}"
    exit 1
fi

if grep -q "rtk git" "$GEMINI_MD" && ! grep -q "OLD RTK CONTENT" "$GEMINI_MD"; then
    echo -e "${GREEN}✅ RTK block updated correctly${NC}"
else
    echo -e "${RED}❌ RTK block update failed${NC}"
    exit 1
fi

echo "2b. Testing settings.json Hook Registration..."
if grep -q '"name": "rtk-rewrite"' "$GEMINI_DIR/settings.json"; then
    echo -e "${GREEN}✅ Hook registered in settings.json${NC}"
else
    echo -e "${RED}❌ Hook NOT registered in settings.json${NC}"
    exit 1
fi

echo "3. Testing Uninstall..."
$RTK_BIN init -g --uninstall > /dev/null

if [ ! -f "$GEMINI_MD" ] && [ ! -f "$RTK_MD" ] && [ ! -f "$HOOK_PATH" ]; then
    echo -e "${GREEN}✅ Uninstall cleaned up all Gemini files${NC}"
else
    echo -e "${RED}❌ Uninstall failed to clean up${NC}"
    exit 1
fi

echo ""
echo -e "${GREEN}✨ ALL GEMINI INIT TESTS PASSED ✨${NC}"

# Cleanup
rm -rf "$TEST_HOME"
