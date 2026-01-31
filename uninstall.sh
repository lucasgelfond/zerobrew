#!/bin/bash
set -e

# zerobrew installer
# Usage: curl -sSL https://raw.githubusercontent.com/lucasgelfond/zerobrew/main/uninstall.sh | bash

: ${ZEROBREW_DIR:=$HOME/.zerobrew}
: ${ZEROBREW_BIN:=$HOME/.local/bin}

if [[ -d "/opt/zerobrew" ]]; then
    ZEROBREW_ROOT="/opt/zerobrew"
elif [[ "$(uname -s)" == "Darwin" ]]; then
    ZEROBREW_ROOT="/opt/zerobrew"
else
    XDG_DATA_HOME="${XDG_DATA_HOME:-$HOME/.local/share}"
    ZEROBREW_ROOT="$XDG_DATA_HOME/zerobrew"
fi

echo "Uninstalling zerobrew..."

if [[ -f "$ZEROBREW_BIN/zb" ]]; then
    "$ZEROBREW_BIN/zb" reset --yes

    rm "$ZEROBREW_BIN/zb"
    echo "Removed binary from $ZEROBREW_BIN/zb"
else
    echo "ERROR: zerobrew is not installed on your system!"
    exit 1
fi

if [[ -d "$ZEROBREW_BIN" ]] && [[ -z "$(ls -A "$ZEROBREW_BIN")" ]]; then
    rmdir "$ZEROBREW_BIN"
    echo "Removed empty directory $ZEROBREW_BIN"
fi

if [[ -d "$ZEROBREW_DIR" ]]; then
    rm -rf "$ZEROBREW_DIR"
    echo "Removed source directory $ZEROBREW_DIR"
fi

if [[ -d "$ZEROBREW_ROOT" ]]; then
    if [[ "$ZEROBREW_ROOT" == /opt/* ]]; then
        echo "Removing $ZEROBREW_ROOT (requires sudo)..."
        sudo rm -rf "$ZEROBREW_ROOT"
    else
        rm -rf "$ZEROBREW_ROOT"
    fi
    echo "Removed root directory $ZEROBREW_ROOT"
fi

case "$SHELL" in
    */zsh)
        ZDOTDIR="${ZDOTDIR:-$HOME}"
        SHELL_CONFIG=$([[ -f "$ZDOTDIR/.zshenv" ]] && echo "$ZDOTDIR/.zshenv" || echo "$ZDOTDIR/.zshrc")
        ;;
    */bash)
        SHELL_CONFIG=$([[ -f "$HOME/.bash_profile" ]] && echo "$HOME/.bash_profile" || echo "$HOME/.bashrc")
        ;;
    *)
        SHELL_CONFIG="$HOME/.profile"
        ;;
esac

if [[ -f "$SHELL_CONFIG" ]]; then
    if grep -q "# zerobrew" "$SHELL_CONFIG"; then
        echo "Cleaning up $SHELL_CONFIG..."
        
        sed -i '/# zerobrew/,/export PATH=.*zerobrew.*PATH"/d' "$SHELL_CONFIG"
        sed -i '/_zb_path_append/d' "$SHELL_CONFIG"
        sed -i '/ZEROBREW_/d' "$SHELL_CONFIG"
        
        echo "Removed environment variables from $SHELL_CONFIG"
    fi
fi

echo ""
echo "============================================"
echo "  zerobrew uninstalled successfully!"
echo "============================================"
echo ""
echo "Note: You may need to restart your terminal or manually"
echo "unset ZEROBREW variables from your current session."
echo ""
