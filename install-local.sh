#!/bin/bash
set -e

# Local zerobrew installer
# Run from the zerobrew source directory to build and install

ZEROBREW_BIN="$HOME/.local/bin"

echo "Building zerobrew..."

# Check for Rust/Cargo
if ! command -v cargo &> /dev/null; then
    echo "Error: Cargo not found. Please install Rust first:"
    echo "  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh"
    exit 1
fi

echo "Rust version: $(rustc --version)"

# Build release binary
cargo build --release

# Create bin directory and install binary
mkdir -p "$ZEROBREW_BIN"
cp target/release/zb "$ZEROBREW_BIN/zb"
chmod +x "$ZEROBREW_BIN/zb"
echo "Installed zb to $ZEROBREW_BIN/zb"

# Detect shell config file
case "$SHELL" in
    */zsh)
        SHELL_CONFIG="$HOME/.zshrc"
        ;;
    */bash)
        if [[ -f "$HOME/.bash_profile" ]]; then
            SHELL_CONFIG="$HOME/.bash_profile"
        else
            SHELL_CONFIG="$HOME/.bashrc"
        fi
        ;;
    *)
        SHELL_CONFIG="$HOME/.profile"
        ;;
esac

# Add to PATH in shell config if not already there
PATHS_TO_ADD=("$ZEROBREW_BIN" "/opt/zerobrew/prefix/bin")
NEEDS_PATH_UPDATE=false

for path_entry in "${PATHS_TO_ADD[@]}"; do
    if ! grep -q "$path_entry" "$SHELL_CONFIG" 2>/dev/null; then
        if [ "$NEEDS_PATH_UPDATE" = false ]; then
            echo "" >> "$SHELL_CONFIG"
            echo "# zerobrew" >> "$SHELL_CONFIG"
            NEEDS_PATH_UPDATE=true
        fi
        echo "export PATH=\"$path_entry:\$PATH\"" >> "$SHELL_CONFIG"
        echo "Added $path_entry to PATH in $SHELL_CONFIG"
    fi
done

# Export for current session so zb init works
export PATH="$ZEROBREW_BIN:/opt/zerobrew/prefix/bin:$PATH"

# Set up /opt/zerobrew directories with correct ownership
echo ""
echo "Setting up zerobrew directories..."
CURRENT_USER=$(whoami)
if [[ ! -d "/opt/zerobrew" ]] || [[ ! -w "/opt/zerobrew" ]]; then
    echo "Creating /opt/zerobrew (requires sudo)..."
    sudo mkdir -p /opt/zerobrew/store /opt/zerobrew/db /opt/zerobrew/cache /opt/zerobrew/locks
    sudo mkdir -p /opt/zerobrew/prefix/bin /opt/zerobrew/prefix/Cellar /opt/zerobrew/prefix/opt
    sudo chown -R "$CURRENT_USER" /opt/zerobrew
fi

# Run zb init to finalize setup
echo ""
echo "Running zb init..."
"$ZEROBREW_BIN/zb" init

echo ""
echo "============================================"
echo "  zerobrew installed successfully!"
echo "============================================"
echo ""

# Check if paths are already in current PATH
if [[ ":$PATH:" != *":$ZEROBREW_BIN:"* ]] || [[ ":$PATH:" != *":/opt/zerobrew/prefix/bin:"* ]]; then
    echo "Run this to start using zerobrew now:"
    echo ""
    echo "    export PATH=\"$ZEROBREW_BIN:/opt/zerobrew/prefix/bin:\$PATH\""
    echo ""
    echo "Or restart your terminal."
    echo ""
fi

echo "Then try: zb install ffmpeg"
echo ""
