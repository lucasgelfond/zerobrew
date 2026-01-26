#!/bin/bash
set -e

# zerobrew installer
# Usage: curl -sSL https://raw.githubusercontent.com/lucasgelfond/zerobrew/main/install.sh | bash

ZEROBREW_REPO="https://github.com/lucasgelfond/zerobrew.git"
ZEROBREW_DIR="$HOME/.zerobrew"
ZEROBREW_BIN="$HOME/.local/bin"

echo "Installing zerobrew..."

# Check for Rust/Cargo
if ! command -v cargo &> /dev/null; then
    echo "Rust not found. Installing via rustup..."
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
    source "$HOME/.cargo/env"
fi

# Ensure cargo is available
if ! command -v cargo &> /dev/null; then
    echo "Error: Cargo still not found after installing Rust"
    exit 1
fi

echo "Rust version: $(rustc --version)"

# Clone or update repo
if [[ -d "$ZEROBREW_DIR" ]]; then
    echo "Updating zerobrew..."
    cd "$ZEROBREW_DIR"
    git pull
else
    echo "Cloning zerobrew..."
    git clone "$ZEROBREW_REPO" "$ZEROBREW_DIR"
    cd "$ZEROBREW_DIR"
fi

# Build
echo "Building zerobrew..."
cargo build --release

# Create bin directory and install binary
mkdir -p "$ZEROBREW_BIN"
cp target/release/zb "$ZEROBREW_BIN/zb"
chmod +x "$ZEROBREW_BIN/zb"
echo "Installed zb to $ZEROBREW_BIN/zb"

# Detect shell config file
case "$SHELL" in
    */zsh)
        ZDOTDIR="${ZDOTDIR:-$HOME}"
        if [[ -f "$ZDOTDIR/.zshenv" ]]; then
            SHELL_CONFIG="$ZDOTDIR/.zshenv"
        else
            SHELL_CONFIG="$ZDOTDIR/.zshrc"
        fi
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
for path_entry in "${PATHS_TO_ADD[@]}"; do
    if ! grep -q "$path_entry" "$SHELL_CONFIG" 2>/dev/null; then
        echo "" >> "$SHELL_CONFIG"
        echo "# zerobrew" >> "$SHELL_CONFIG"
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
    sudo mkdir -p /opt/zerobrew/prefix/bin /opt/zerobrew/prefix/Cellar
    sudo chown -R "$CURRENT_USER" /opt/zerobrew
    sudo chown -R "$CURRENT_USER" /opt/zerobrew/prefix
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
echo "Run this to start using zerobrew now:"
echo ""
echo "    export PATH=\"$ZEROBREW_BIN:/opt/zerobrew/prefix/bin:\$PATH\""
echo ""
echo "Or restart your terminal."
echo ""
echo "Then try: zb install ffmpeg"
echo ""
