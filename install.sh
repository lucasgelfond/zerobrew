#!/bin/bash
set -e

# zerobrew installer
# Usage: curl -sSL https://raw.githubusercontent.com/lucasgelfond/zerobrew/main/install.sh | bash

ZEROBREW_REPO="https://github.com/lucasgelfond/zerobrew.git"
: ${ZEROBREW_SRC:=$HOME/.zerobrew}
: ${ZEROBREW_BIN:=$HOME/.local/bin}
: ${ZEROBREW_ROOT:=/opt/zerobrew}
: ${ZEROBREW_PREFIX:=$ZEROBREW_ROOT/prefix}

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
if [[ -d "$ZEROBREW_SRC" ]]; then
    echo "Updating zerobrew..."
    cd "$ZEROBREW_SRC"
    git fetch --depth=1 origin main
    git reset --hard origin/main
else
    echo "Cloning zerobrew..."
    git clone --depth 1 "$ZEROBREW_REPO" "$ZEROBREW_SRC"
    cd "$ZEROBREW_SRC"
fi

# Build
echo "Building zerobrew..."
if [[ -d "$ZEROBREW_PREFIX/lib/pkgconfig" ]]; then
    export PKG_CONFIG_PATH="$ZEROBREW_PREFIX/lib/pkgconfig:${PKG_CONFIG_PATH:-}"
fi
if [[ -d "/opt/homebrew/lib/pkgconfig" ]] && [[ ! "$PKG_CONFIG_PATH" =~ "/opt/homebrew/lib/pkgconfig" ]]; then
    export PKG_CONFIG_PATH="/opt/homebrew/lib/pkgconfig:${PKG_CONFIG_PATH:-}"
fi
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

if [[ ! -w $SHELL_CONFIG ]]; then
    echo "Error, config not writable: $SHELL_CONFIG" >&2
    exit 1
fi

# Add zb binary to PATH (the main ZEROBREW_ROOT/PREFIX config is handled by `zb init`)
if ! grep -q "$ZEROBREW_BIN" "$SHELL_CONFIG" 2>/dev/null; then
    cat >>"$SHELL_CONFIG" <<EOF

# zerobrew cli
export PATH="$ZEROBREW_BIN:\$PATH"
EOF
    echo "Added $ZEROBREW_BIN to PATH in $SHELL_CONFIG"
fi

# Export for current session so zb init works
export PATH="$ZEROBREW_BIN:$PATH"

# Run zb init to set up directories and configure ZEROBREW_ROOT/ZEROBREW_PREFIX
echo ""
echo "Running zb init..."
echo "(You can customize the installation directory during init)"
echo ""

# If running via pipe (curl | bash), try to reconnect to TTY for interactive prompts
if [ ! -t 0 ] && [ -e /dev/tty ]; then
    "$ZEROBREW_BIN/zb" init < /dev/tty
else
    "$ZEROBREW_BIN/zb" init
fi

echo ""
echo "============================================"
echo "  zerobrew installed successfully!"
echo "============================================"
echo ""
echo "Run this to start using zerobrew now:"
echo ""
echo "    source $SHELL_CONFIG"
echo ""
echo "Or restart your terminal."
echo ""
echo "Then try: zb install ffmpeg"
echo ""
