#!/bin/bash
# Apply abrowser patches to Chromium source
set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(dirname "$SCRIPT_DIR")"
CHROMIUM_SRC="${ROOT_DIR}/chromium/src"

if [ ! -d "$CHROMIUM_SRC" ]; then
    echo "Error: Chromium source not found at $CHROMIUM_SRC"
    echo "Run 'gclient sync' in the chromium directory first."
    exit 1
fi

echo "Applying abrowser patches to Chromium..."

cd "$CHROMIUM_SRC"

# Apply each patch in order
for patch in "$ROOT_DIR/chromium/patches/chromium"/*.patch; do
    echo "Applying $(basename "$patch")..."
    git apply --check "$patch" 2>/dev/null || {
        echo "  Patch may already be applied or conflicts exist, trying with 3-way merge..."
        git apply --3way "$patch" || {
            echo "  Warning: Could not apply $patch cleanly"
        }
    }
    git apply "$patch" 2>/dev/null || true
done

echo "Done! Patches applied."
