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

# abrowser version
ABROWSER_VERSION="0.1.0"

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
                build-essential clang lld ninja-build \
                pkg-config \
                libglib2.0-dev libgtk-3-dev \
                libnss3-dev libatk1.0-dev libatk-bridge2.0-dev \
                libcups2-dev libdrm-dev libdrm2 mesa-common-dev libxkbcommon-dev \
                libxcomposite-dev libxdamage-dev libxrandr-dev \
                libgbm-dev libasound2-dev libpango1.0-dev \
                libcairo2-dev libfontconfig1-dev \
                libsecret-1-dev \
                libxss-dev libxtst-dev \
                libpulse-dev libudev-dev \
                libva-dev libcurl4-openssl-dev \
                libx11-dev libxcb1-dev libpci-dev
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
        echo "depot_tools already exists"
    else
        echo "Cloning depot_tools..."
        git clone https://chromium.googlesource.com/chromium/tools/depot_tools.git "$DEPOT_TOOLS_DIR"
    fi

    export PATH="$DEPOT_TOOLS_DIR:$PATH"

    # Ensure depot_tools is initialized
    if [ ! -f "$DEPOT_TOOLS_DIR/python3_bin_reldir.txt" ]; then
        echo "Initializing depot_tools..."
        cd "$DEPOT_TOOLS_DIR"
        ./update_depot_tools
        cd "$SCRIPT_DIR"
    fi

    echo "depot_tools OK"
    echo ""
}

# =============================================================================
# Step 2: Get Chromium sources
# =============================================================================
get_chromium() {
    echo "=== Step 2: Getting Chromium sources ==="

    if [ -d "$CHROMIUM_SRC" ]; then
        echo "Chromium sources already exist, skipping"
        echo ""
        return
    fi

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

    echo "Fetching Chromium (this may take a while)..."
    gclient sync --no-history --shallow

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

    if [ "$OS" = "Darwin" ]; then
        GN_ARGS+="
enable_swiftshader=false
angle_enable_swiftshader=false
enable_swiftshader_vulkan=false
angle_enable_vulkan=false
use_dawn=false
use_crashpad=false
"
    elif [ "$OS" = "Linux" ]; then
        GN_ARGS+="
use_sysroot=false
use_glib=true
use_gnome_keyring=false
use_qt=false
use_system_libdrm=true
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
# Step 7: Create distribution package
# =============================================================================
create_package() {
    echo "=== Step 7: Creating distribution package ==="

    DIST_DIR="$SCRIPT_DIR/dist"
    mkdir -p "$DIST_DIR"

    if [ "$OS" = "Darwin" ]; then
        # macOS - create .dmg
        PKG_NAME="abrowser-${ABROWSER_VERSION}-macos-${ARCH}"
        APP_DIR="$DIST_DIR/$PKG_NAME"

        rm -rf "$APP_DIR"
        mkdir -p "$APP_DIR"

        # Copy binary and required files
        cp "$CHROMIUM_SRC/out/Release/headless_shell" "$APP_DIR/abrowser"

        # Copy required frameworks/dylibs
        if [ -d "$CHROMIUM_SRC/out/Release/Frameworks" ]; then
            cp -R "$CHROMIUM_SRC/out/Release/Frameworks" "$APP_DIR/"
        fi

        # Copy v8 snapshots if present
        for f in "$CHROMIUM_SRC/out/Release"/*.bin; do
            [ -f "$f" ] && cp "$f" "$APP_DIR/"
        done

        # Copy pak files
        for f in "$CHROMIUM_SRC/out/Release"/*.pak; do
            [ -f "$f" ] && cp "$f" "$APP_DIR/"
        done

        # Create DMG
        DMG_PATH="$DIST_DIR/${PKG_NAME}.dmg"
        rm -f "$DMG_PATH"
        hdiutil create -volname "abrowser" -srcfolder "$APP_DIR" -ov -format UDZO "$DMG_PATH"

        rm -rf "$APP_DIR"
        echo "Package created: $DMG_PATH"

    elif [ "$OS" = "Linux" ]; then
        # Linux - create .deb
        PKG_NAME="abrowser-${ABROWSER_VERSION}-linux-${ARCH}"
        DEB_DIR="$DIST_DIR/${PKG_NAME}"

        rm -rf "$DEB_DIR"
        mkdir -p "$DEB_DIR/DEBIAN"
        mkdir -p "$DEB_DIR/usr/bin"
        mkdir -p "$DEB_DIR/usr/lib/abrowser"
        mkdir -p "$DEB_DIR/usr/share/applications"

        # Control file
        cat > "$DEB_DIR/DEBIAN/control" << EOF
Package: abrowser
Version: ${ABROWSER_VERSION}
Section: web
Priority: optional
Architecture: $(dpkg --print-architecture)
Depends: libnss3, libatk1.0-0, libatk-bridge2.0-0, libcups2, libdrm2, libxkbcommon0, libxcomposite1, libxdamage1, libxrandr2, libgbm1, libasound2, libpango-1.0-0, libcairo2, libsecret-1-0
Maintainer: abrowser
Description: Accessible terminal web browser
 A terminal-based accessible web browser built on Chromium headless.
EOF

        # Copy binary
        cp "$CHROMIUM_SRC/out/Release/headless_shell" "$DEB_DIR/usr/lib/abrowser/abrowser"
        chmod 755 "$DEB_DIR/usr/lib/abrowser/abrowser"

        # Copy required files
        for f in "$CHROMIUM_SRC/out/Release"/*.bin; do
            [ -f "$f" ] && cp "$f" "$DEB_DIR/usr/lib/abrowser/"
        done
        for f in "$CHROMIUM_SRC/out/Release"/*.pak; do
            [ -f "$f" ] && cp "$f" "$DEB_DIR/usr/lib/abrowser/"
        done
        if [ -d "$CHROMIUM_SRC/out/Release/locales" ]; then
            cp -R "$CHROMIUM_SRC/out/Release/locales" "$DEB_DIR/usr/lib/abrowser/"
        fi

        # Create wrapper script
        cat > "$DEB_DIR/usr/bin/abrowser" << 'EOF'
#!/bin/bash
exec /usr/lib/abrowser/abrowser "$@"
EOF
        chmod 755 "$DEB_DIR/usr/bin/abrowser"

        # Desktop file
        cat > "$DEB_DIR/usr/share/applications/abrowser.desktop" << EOF
[Desktop Entry]
Name=abrowser
Comment=Accessible terminal web browser
Exec=abrowser %U
Terminal=true
Type=Application
Categories=Network;WebBrowser;
EOF

        # Build deb
        DEB_PATH="$DIST_DIR/${PKG_NAME}.deb"
        dpkg-deb --build "$DEB_DIR" "$DEB_PATH"

        rm -rf "$DEB_DIR"
        echo "Package created: $DEB_PATH"
    fi

    echo ""
}

# =============================================================================
# Main
# =============================================================================

# Parse arguments
SKIP_DEPS=false
SKIP_FETCH=false
SKIP_RUST=false
SKIP_PACKAGE=false

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
        --skip-package)
            SKIP_PACKAGE=true
            shift
            ;;
        --help)
            echo "Usage: $0 [options]"
            echo ""
            echo "Options:"
            echo "  --skip-deps    Skip dependency installation"
            echo "  --skip-fetch   Skip Chromium fetch (use existing sources)"
            echo "  --skip-rust    Skip Rust library build"
            echo "  --skip-package Skip distribution package creation"
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

if [ "$SKIP_PACKAGE" = false ]; then
    create_package
fi
