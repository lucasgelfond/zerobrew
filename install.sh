#!/bin/bash
set -e

# Detect environment
if [[ "$(uname --operating-system 2>/dev/null)" == "Android" && -n "$TERMUX_VERSION" ]]; then
    ENV_PREFIX=$PREFIX
    SUDO_CMD=""
else
    ENV_PREFIX=""
    SUDO_CMD=sudo
fi

# zerobrew installer
# Usage: curl -sSL https://raw.githubusercontent.com/lucasgelfond/zerobrew/main/install.sh | bash

ZEROBREW_REPO="https://github.com/lucasgelfond/zerobrew.git"
: ${ZEROBREW_DIR:=$HOME/.zerobrew}
: ${ZEROBREW_BIN:=$HOME/.local/bin}

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
    git fetch --depth=1 origin main
    git reset --hard origin/main
else
    echo "Cloning zerobrew..."
    git clone --depth 1 "$ZEROBREW_REPO" "$ZEROBREW_DIR"
    cd "$ZEROBREW_DIR"
fi

# Build
echo "Building zerobrew..."
if [[ -d "$ENV_PREFIX/opt/zerobrew/prefix/lib/pkgconfig" ]]; then
    export PKG_CONFIG_PATH="$ENV_PREFIX/opt/zerobrew/prefix/lib/pkgconfig:${PKG_CONFIG_PATH:-}"
fi
if [[ -d "$ENV_PREFIX/opt/homebrew/lib/pkgconfig" ]] && [[ ! "$PKG_CONFIG_PATH" =~ "$ENV_PREFIX/opt/homebrew/lib/pkgconfig" ]]; then
    export PKG_CONFIG_PATH="$ENV_PREFIX/opt/homebrew/lib/pkgconfig:${PKG_CONFIG_PATH:-}"
fi
ZB_ROOT="$ENV_PREFIX/opt/zerobrew" ZB_PREFIX="$ENV_PREFIX/opt/zerobrew/prefix" cargo build --release

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

# Add to PATH in shell config if not already there
PATHS_TO_ADD=("$ZEROBREW_BIN" "$ENV_PREFIX/opt/zerobrew/prefix/bin")
if ! grep -q "^# zerobrew$" "$SHELL_CONFIG" 2>/dev/null; then
    cat >>"$SHELL_CONFIG" <<EOF
# zerobrew
export ZEROBREW_DIR=$ZEROBREW_DIR
export ZEROBREW_BIN=$ZEROBREW_BIN
export PKG_CONFIG_PATH="$ENV_PREFIX/opt/zerobrew/prefix/lib/pkgconfig:\${PKG_CONFIG_PATH:-}"
_zb_path_append() {
    local argpath="\$1"
    case ":\${PATH}:" in
        *:"\$argpath":*) ;;
        *) export PATH="\$argpath:\$PATH" ;;
    esac;
}
EOF
    for path_entry in "${PATHS_TO_ADD[@]}"; do
        if ! grep -q "_zb_path_append $path_entry" "$SHELL_CONFIG" 2>/dev/null; then
            echo "_zb_path_append $path_entry" >>"$SHELL_CONFIG"
            echo "Added $path_entry to PATH in $SHELL_CONFIG"
        fi
    done
fi

# Export for current session so zb init works
export PATH="$ZEROBREW_BIN:$ENV_PREFIX/opt/zerobrew/prefix/bin:$PATH"

# Set up $ENV_PREFIX/opt/zerobrew directories with correct ownership
echo ""
echo "Setting up zerobrew directories..."
CURRENT_USER=$(whoami)
if [[ ! -d "$ENV_PREFIX/opt/zerobrew" ]] || [[ ! -w "$ENV_PREFIX/opt/zerobrew" ]]; then
    if [[ -n $SUDO_CMD ]]; then
        echo "Creating /opt/zerobrew (requires sudo)..."
    else
        echo "Creating $ENV_PREFIX/opt/zerobrew..."
    fi

    $SUDO_CMD mkdir -p $ENV_PREFIX/opt/zerobrew/store $ENV_PREFIX/opt/zerobrew/db $ENV_PREFIX/opt/zerobrew/cache $ENV_PREFIX/opt/zerobrew/locks
    $SUDO_CMD mkdir -p $ENV_PREFIX/opt/zerobrew/prefix/bin $ENV_PREFIX/opt/zerobrew/prefix/Cellar
    $SUDO_CMD chown -R "$CURRENT_USER" $ENV_PREFIX/opt/zerobrew
    $SUDO_CMD chown -R "$CURRENT_USER" $ENV_PREFIX/opt/zerobrew/prefix
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
echo "    export PATH=\"$ZEROBREW_BIN:$ENV_PREFIX/opt/zerobrew/prefix/bin:\$PATH\""
echo ""
echo "Or restart your terminal, to source updated ${SHELL_CONFIG}."
echo ""
echo "Then try: zb install ffmpeg"
echo ""
