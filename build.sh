#!/bin/bash
set -e

# abrowser build script
# Builds the abrowser Rust binary and checks for Chrome/Chromium dependency

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

# abrowser version
ABROWSER_VERSION="0.1.0"

# Detect OS
OS=$(uname -s)
ARCH=$(uname -m)

echo "=== abrowser build ==="
echo "OS: $OS"
echo "Architecture: $ARCH"
echo ""

# =============================================================================
# Check for Chrome/Chromium
# =============================================================================
check_chrome() {
    echo "=== Checking for Chrome/Chromium ==="

    CHROME_FOUND=false
    CHROME_PATH=""

    if [ "$OS" = "Darwin" ]; then
        # macOS
        if [ -x "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome" ]; then
            CHROME_FOUND=true
            CHROME_PATH="/Applications/Google Chrome.app/Contents/MacOS/Google Chrome"
        elif [ -x "/Applications/Chromium.app/Contents/MacOS/Chromium" ]; then
            CHROME_FOUND=true
            CHROME_PATH="/Applications/Chromium.app/Contents/MacOS/Chromium"
        fi
    elif [ "$OS" = "Linux" ]; then
        # Linux - check common locations
        for binary in google-chrome google-chrome-stable chromium chromium-browser; do
            if command -v "$binary" &>/dev/null; then
                CHROME_FOUND=true
                CHROME_PATH=$(command -v "$binary")
                break
            fi
        done
        # Also check standard paths
        for path in /usr/bin/google-chrome /usr/bin/chromium /usr/bin/chromium-browser; do
            if [ -x "$path" ]; then
                CHROME_FOUND=true
                CHROME_PATH="$path"
                break
            fi
        done
    fi

    if [ "$CHROME_FOUND" = true ]; then
        echo "Found: $CHROME_PATH"
        # Get version
        if [ "$OS" = "Darwin" ]; then
            VERSION=$("$CHROME_PATH" --version 2>/dev/null | head -1 || echo "unknown")
        else
            VERSION=$("$CHROME_PATH" --version 2>/dev/null || echo "unknown")
        fi
        echo "Version: $VERSION"
        echo ""
        return 0
    else
        echo ""
        echo "Chrome/Chromium not found!"
        echo ""

        if [ "$OS" = "Darwin" ]; then
            echo "Would you like to install Google Chrome? (requires Homebrew)"
            echo ""
            echo "  1) Install via Homebrew (brew install --cask google-chrome)"
            echo "  2) Skip (I'll install manually)"
            echo ""
            read -p "Choice [1/2]: " choice

            if [ "$choice" = "1" ]; then
                if command -v brew &>/dev/null; then
                    echo "Installing Google Chrome..."
                    brew install --cask google-chrome
                    echo ""
                    echo "Chrome installed successfully!"
                    return 0
                else
                    echo "Homebrew not found. Please install from https://brew.sh"
                    echo "Then run: brew install --cask google-chrome"
                    return 1
                fi
            else
                echo ""
                echo "Please install Chrome manually:"
                echo "  - Download from https://www.google.com/chrome/"
                echo "  - Or run: brew install --cask google-chrome"
                return 1
            fi

        elif [ "$OS" = "Linux" ]; then
            echo "Would you like to install Chromium?"
            echo ""
            echo "  1) Install via apt (sudo apt install chromium)"
            echo "  2) Skip (I'll install manually)"
            echo ""
            read -p "Choice [1/2]: " choice

            if [ "$choice" = "1" ]; then
                if command -v apt-get &>/dev/null; then
                    echo "Installing Chromium..."
                    sudo apt-get update
                    sudo apt-get install -y chromium || sudo apt-get install -y chromium-browser
                    echo ""
                    echo "Chromium installed successfully!"
                    return 0
                else
                    echo "apt not found. Please install Chromium manually for your distribution."
                    return 1
                fi
            else
                echo ""
                echo "Please install Chrome/Chromium manually:"
                echo "  - Debian/Ubuntu: sudo apt install chromium"
                echo "  - Fedora: sudo dnf install chromium"
                echo "  - Arch: sudo pacman -S chromium"
                return 1
            fi
        else
            echo "Please install Google Chrome or Chromium for your platform."
            return 1
        fi
    fi
}

# =============================================================================
# Install Rust dependencies
# =============================================================================
check_rust() {
    echo "=== Checking Rust toolchain ==="

    if ! command -v cargo &>/dev/null; then
        echo "Rust not found. Please install from https://rustup.rs"
        exit 1
    fi

    echo "Rust: $(rustc --version)"
    echo "Cargo: $(cargo --version)"
    echo ""
}

# =============================================================================
# Build abrowser
# =============================================================================
build_abrowser() {
    echo "=== Building abrowser ==="

    cd "$SCRIPT_DIR"

    # Build release binary
    cargo build --release

    echo ""
    echo "Build complete!"
    echo "Binary: $SCRIPT_DIR/target/release/abrowser"
    echo ""
}

# =============================================================================
# Create distribution package (optional)
# =============================================================================
create_package() {
    echo "=== Creating distribution package ==="

    DIST_DIR="$SCRIPT_DIR/dist"
    mkdir -p "$DIST_DIR"

    if [ "$OS" = "Darwin" ]; then
        PKG_NAME="abrowser-${ABROWSER_VERSION}-macos-${ARCH}"

        # Simple tar.gz for macOS
        cd "$SCRIPT_DIR/target/release"
        tar -czf "$DIST_DIR/${PKG_NAME}.tar.gz" abrowser

        echo "Package created: $DIST_DIR/${PKG_NAME}.tar.gz"
        echo ""
        echo "To install:"
        echo "  tar -xzf ${PKG_NAME}.tar.gz"
        echo "  sudo mv abrowser /usr/local/bin/"

    elif [ "$OS" = "Linux" ]; then
        PKG_NAME="abrowser-${ABROWSER_VERSION}-linux-${ARCH}"
        DEB_DIR="$DIST_DIR/${PKG_NAME}"

        rm -rf "$DEB_DIR"
        mkdir -p "$DEB_DIR/DEBIAN"
        mkdir -p "$DEB_DIR/usr/bin"

        # Control file
        cat > "$DEB_DIR/DEBIAN/control" << EOF
Package: abrowser
Version: ${ABROWSER_VERSION}
Section: web
Priority: optional
Architecture: $(dpkg --print-architecture 2>/dev/null || echo "amd64")
Depends: chromium | chromium-browser | google-chrome-stable
Maintainer: abrowser
Description: Accessible terminal web browser
 A terminal-based accessible web browser that uses Chrome/Chromium
 in headless mode for rendering.
EOF

        # Copy binary
        cp "$SCRIPT_DIR/target/release/abrowser" "$DEB_DIR/usr/bin/"
        chmod 755 "$DEB_DIR/usr/bin/abrowser"

        # Build deb
        if command -v dpkg-deb &>/dev/null; then
            DEB_PATH="$DIST_DIR/${PKG_NAME}.deb"
            dpkg-deb --build "$DEB_DIR" "$DEB_PATH"
            rm -rf "$DEB_DIR"
            echo "Package created: $DEB_PATH"
        else
            # Fallback to tar.gz
            cd "$SCRIPT_DIR/target/release"
            tar -czf "$DIST_DIR/${PKG_NAME}.tar.gz" abrowser
            rm -rf "$DEB_DIR"
            echo "Package created: $DIST_DIR/${PKG_NAME}.tar.gz"
        fi
    fi

    echo ""
}

# =============================================================================
# Main
# =============================================================================

SKIP_CHROME_CHECK=false
CREATE_PACKAGE=false

while [[ $# -gt 0 ]]; do
    case $1 in
        --skip-chrome-check)
            SKIP_CHROME_CHECK=true
            shift
            ;;
        --package)
            CREATE_PACKAGE=true
            shift
            ;;
        --help)
            echo "Usage: $0 [options]"
            echo ""
            echo "Options:"
            echo "  --skip-chrome-check  Skip Chrome/Chromium check"
            echo "  --package            Create distribution package"
            echo "  --help               Show this help"
            echo ""
            echo "Requirements:"
            echo "  - Rust toolchain (https://rustup.rs)"
            echo "  - Google Chrome or Chromium browser"
            exit 0
            ;;
        *)
            echo "Unknown option: $1"
            exit 1
            ;;
    esac
done

# Check dependencies
check_rust

if [ "$SKIP_CHROME_CHECK" = false ]; then
    if ! check_chrome; then
        echo ""
        echo "Chrome/Chromium is required for abrowser to function."
        echo "Please install it and re-run this script, or use --skip-chrome-check"
        exit 1
    fi
fi

# Build
build_abrowser

# Package if requested
if [ "$CREATE_PACKAGE" = true ]; then
    create_package
fi

echo "=== Done ==="
echo ""
echo "Run abrowser with:"
echo "  ./target/release/abrowser [url]"
echo ""
