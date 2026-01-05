#!/bin/bash
set -e

# abrowser build script
# Builds patched Chromium headless_shell with abrowser Rust library
# Works on macOS and Debian/Ubuntu

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
CHROMIUM_DIR="$SCRIPT_DIR/chromium"
CHROMIUM_SRC="$CHROMIUM_DIR/src"
PATCHES_DIR="$SCRIPT_DIR/chromium/patches/chromium"
DEPOT_TOOLS_DIR="$SCRIPT_DIR/depot_tools"

# Chromium version to build against
CHROMIUM_VERSION="120.0.6099.224"

# Detect OS
OS=$(uname -s)
ARCH=$(uname -m)

echo "=== abrowser build ==="
echo "OS: $OS"
echo "Architecture: $ARCH"
echo "Chromium version: $CHROMIUM_VERSION"
echo ""

# =============================================================================
# Step 0: Install system dependencies
# =============================================================================
install_dependencies() {
    echo "=== Step 0: Checking/installing dependencies ==="

    if [ "$OS" = "Darwin" ]; then
        # macOS - check for Xcode command line tools
        if ! xcode-select -p &>/dev/null; then
            echo "Installing Xcode command line tools..."
            xcode-select --install
            echo "Please re-run this script after Xcode tools are installed."
            exit 1
        fi

        # Check for Homebrew and install if needed
        if ! command -v brew &>/dev/null; then
            echo "Homebrew not found. Please install it from https://brew.sh"
            exit 1
        fi

        # Install required packages
        brew list python@3 &>/dev/null || brew install python@3
        brew list git &>/dev/null || brew install git

    elif [ "$OS" = "Linux" ]; then
        # Debian/Ubuntu
        if command -v apt-get &>/dev/null; then
            echo "Installing build dependencies..."
            sudo apt-get update
            sudo apt-get install -y \
                git python3 python3-pip curl wget \
                build-essential clang lld \
                libglib2.0-dev libgtk-3-dev \
                libnss3-dev libatk1.0-dev libatk-bridge2.0-dev \
                libcups2-dev libdrm-dev libxkbcommon-dev \
                libxcomposite-dev libxdamage-dev libxrandr-dev \
                libgbm-dev libasound2-dev libpango1.0-dev \
                libcairo2-dev libfontconfig1-dev
        else
            echo "Unsupported Linux distribution. Please install dependencies manually."
            exit 1
        fi
    else
        echo "Unsupported OS: $OS"
        exit 1
    fi

    echo "Dependencies OK"
    echo ""
}

# =============================================================================
# Step 1: Get depot_tools
# =============================================================================
get_depot_tools() {
    echo "=== Step 1: Getting depot_tools ==="

    if [ -d "$DEPOT_TOOLS_DIR" ]; then
        echo "depot_tools already exists, updating..."
        cd "$DEPOT_TOOLS_DIR"
        git pull --quiet
    else
        echo "Cloning depot_tools..."
        git clone https://chromium.googlesource.com/chromium/tools/depot_tools.git "$DEPOT_TOOLS_DIR"
    fi

    export PATH="$DEPOT_TOOLS_DIR:$PATH"
    echo "depot_tools OK"
    echo ""
}

# =============================================================================
# Step 2: Get Chromium sources
# =============================================================================
get_chromium() {
    echo "=== Step 2: Getting Chromium sources ==="

    mkdir -p "$CHROMIUM_DIR"
    cd "$CHROMIUM_DIR"

    if [ ! -f ".gclient" ]; then
        echo "Creating .gclient configuration..."
        cat > .gclient << EOF
solutions = [
  {
    "name": "src",
    "url": "https://chromium.googlesource.com/chromium/src.git@$CHROMIUM_VERSION",
    "managed": False,
    "custom_deps": {},
    "custom_vars": {},
  },
]
EOF
    fi

    if [ ! -d "src" ]; then
        echo "Fetching Chromium (this may take a while)..."
        gclient sync --no-history --shallow
    else
        echo "Chromium sources already exist"
        # Check version
        cd src
        CURRENT_VERSION=$(git describe --tags --always 2>/dev/null || echo "unknown")
        echo "Current version: $CURRENT_VERSION"
        cd ..
    fi

    echo "Chromium sources OK"
    echo ""
}

# =============================================================================
# Step 3: Apply patches
# =============================================================================
apply_patches() {
    echo "=== Step 3: Applying patches ==="

    cd "$CHROMIUM_SRC"

    # Create abrowser symlinks if not exist
    if [ ! -d "abrowser" ]; then
        mkdir -p abrowser
        ln -sf "../../../src" "abrowser/src"
        ln -sf "../../../build" "abrowser/build"
        echo "Created abrowser symlinks"
    fi

    # Apply patches
    for patch in "$PATCHES_DIR"/*.patch; do
        if [ -f "$patch" ]; then
            PATCH_NAME=$(basename "$patch")
            echo "Applying $PATCH_NAME..."

            # Check if already applied
            if git apply --check --reverse "$patch" 2>/dev/null; then
                echo "  Already applied, skipping"
            else
                git apply "$patch" || {
                    echo "  Failed to apply $PATCH_NAME"
                    echo "  Trying with 3-way merge..."
                    git apply --3way "$patch" || {
                        echo "  ERROR: Could not apply patch $PATCH_NAME"
                        exit 1
                    }
                }
                echo "  Applied successfully"
            fi
        fi
    done

    echo "Patches applied"
    echo ""
}

# =============================================================================
# Step 4: Build Rust library
# =============================================================================
build_rust() {
    echo "=== Step 4: Building Rust library ==="

    cd "$SCRIPT_DIR"

    # Detect Rust target
    if [ "$ARCH" = "x86_64" ]; then
        RUST_TARGET="x86_64"
    elif [ "$ARCH" = "arm64" ] || [ "$ARCH" = "aarch64" ]; then
        RUST_TARGET="aarch64"
    else
        echo "Unsupported architecture: $ARCH"
        exit 1
    fi

    if [ "$OS" = "Darwin" ]; then
        RUST_TARGET="${RUST_TARGET}-apple-darwin"
    else
        RUST_TARGET="${RUST_TARGET}-unknown-linux-gnu"
    fi

    echo "Rust target: $RUST_TARGET"

    # Build
    cargo build --release --target "$RUST_TARGET"

    # Copy library to expected location
    mkdir -p "build/$RUST_TARGET/release"
    if [ "$OS" = "Darwin" ]; then
        cp "target/$RUST_TARGET/release/libabrowser.dylib" "build/$RUST_TARGET/release/" 2>/dev/null || \
        cp "target/$RUST_TARGET/release/libabrowser.a" "build/$RUST_TARGET/release/" 2>/dev/null || true
    else
        cp "target/$RUST_TARGET/release/libabrowser.so" "build/$RUST_TARGET/release/" 2>/dev/null || \
        cp "target/$RUST_TARGET/release/libabrowser.a" "build/$RUST_TARGET/release/" 2>/dev/null || true
    fi

    echo "Rust library built"
    echo ""
}

# =============================================================================
# Step 5: Generate Chromium build files
# =============================================================================
generate_build() {
    echo "=== Step 5: Generating Chromium build files ==="

    cd "$CHROMIUM_SRC"

    # GN args - platform specific
    GN_ARGS="
is_debug=false
is_component_build=false
enable_nacl=false
symbol_level=0
blink_symbol_level=0
v8_symbol_level=0
treat_warnings_as_errors=false
"

    if [ "$OS" = "Linux" ]; then
        GN_ARGS+="
use_sysroot=false
use_glib=true
"
    fi

    mkdir -p out/Release
    echo "$GN_ARGS" > out/Release/args.gn

    echo "Running gn gen..."
    gn gen out/Release

    echo "Build files generated"
    echo ""
}

# =============================================================================
# Step 6: Build headless_shell
# =============================================================================
build_chromium() {
    echo "=== Step 6: Building headless_shell ==="
    echo "This may take a while..."

    cd "$CHROMIUM_SRC"

    autoninja -C out/Release headless_shell

    echo ""
    echo "=== Build complete ==="
    echo "Binary: $CHROMIUM_SRC/out/Release/headless_shell"
}

# =============================================================================
# Main
# =============================================================================

# Parse arguments
SKIP_DEPS=false
SKIP_FETCH=false
SKIP_RUST=false

while [[ $# -gt 0 ]]; do
    case $1 in
        --skip-deps)
            SKIP_DEPS=true
            shift
            ;;
        --skip-fetch)
            SKIP_FETCH=true
            shift
            ;;
        --skip-rust)
            SKIP_RUST=true
            shift
            ;;
        --help)
            echo "Usage: $0 [options]"
            echo ""
            echo "Options:"
            echo "  --skip-deps    Skip dependency installation"
            echo "  --skip-fetch   Skip Chromium fetch (use existing sources)"
            echo "  --skip-rust    Skip Rust library build"
            echo "  --help         Show this help"
            exit 0
            ;;
        *)
            echo "Unknown option: $1"
            exit 1
            ;;
    esac
done

# Run build steps
if [ "$SKIP_DEPS" = false ]; then
    install_dependencies
fi

get_depot_tools

if [ "$SKIP_FETCH" = false ]; then
    get_chromium
fi

apply_patches

if [ "$SKIP_RUST" = false ]; then
    build_rust
fi

generate_build
build_chromium
