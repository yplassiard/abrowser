#!/bin/bash
# Build abrowser
set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(dirname "$SCRIPT_DIR")"

# Determine target triple
if [[ "$(uname -m)" == "x86_64" ]]; then
    ARCH="x86_64"
elif [[ "$(uname -m)" == "arm64" ]] || [[ "$(uname -m)" == "aarch64" ]]; then
    ARCH="aarch64"
else
    echo "Unsupported architecture: $(uname -m)"
    exit 1
fi

if [[ "$(uname)" == "Darwin" ]]; then
    TARGET="${ARCH}-apple-darwin"
elif [[ "$(uname)" == "Linux" ]]; then
    TARGET="${ARCH}-unknown-linux-gnu"
else
    echo "Unsupported OS: $(uname)"
    exit 1
fi

echo "Building abrowser Rust library for $TARGET..."

cd "$ROOT_DIR"
mkdir -p "build/$TARGET/release"

# Build Rust library
cargo build --release --target "$TARGET" || cargo build --release

# Copy library to expected location
if [[ "$(uname)" == "Darwin" ]]; then
    cp "target/$TARGET/release/libabrowser.dylib" "build/$TARGET/release/" 2>/dev/null || \
    cp "target/release/libabrowser.dylib" "build/$TARGET/release/"
else
    cp "target/$TARGET/release/libabrowser.so" "build/$TARGET/release/" 2>/dev/null || \
    cp "target/release/libabrowser.so" "build/$TARGET/release/"
fi

# Also create static library
cp "target/$TARGET/release/libabrowser.a" "build/$TARGET/release/" 2>/dev/null || \
cp "target/release/libabrowser.a" "build/$TARGET/release/" || true

echo "Rust library built: build/$TARGET/release/"

# Now build Chromium headless shell
CHROMIUM_SRC="${ROOT_DIR}/chromium/src"

if [ ! -d "$CHROMIUM_SRC" ]; then
    echo ""
    echo "Note: Chromium source not found. To build the full browser:"
    echo "  1. cd chromium && gclient sync"
    echo "  2. ./scripts/apply-patches.sh"
    echo "  3. cd chromium/src && gn gen out/Release"
    echo "  4. autoninja -C out/Release headless_shell"
    exit 0
fi

echo ""
echo "Building Chromium headless_shell..."
cd "$CHROMIUM_SRC"

# Generate build files if needed
if [ ! -f "out/Release/args.gn" ]; then
    echo "Generating build configuration..."
    gn gen out/Release --args='
        is_debug = false
        is_component_build = false
        symbol_level = 0
        enable_nacl = false
        headless_use_embedded_resources = true
    '
fi

# Build
autoninja -C out/Release headless_shell

echo ""
echo "Build complete! Binary at: chromium/src/out/Release/headless_shell"
