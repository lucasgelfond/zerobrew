# vim: set ft=make :

set script-interpreter := ["bash", "-euo", "pipefail"]

build: fmt lint
    cargo build --bin zb

[script]
install: build
    ZEROBREW_BIN="${ZEROBREW_BIN:-$HOME/.local/bin}"
    ZEROBREW_OPT="${ZEROBREW_OPT:-/opt/zerobrew}"

    if [[ -d "$ZEROBREW_OPT/prefix/lib/pkgconfig" ]]; then
        export PKG_CONFIG_PATH="$ZEROBREW_OPT/prefix/lib/pkgconfig:${PKG_CONFIG_PATH:-}"
    fi
    if [[ -d "/opt/homebrew/lib/pkgconfig" ]] && [[ ! "$PKG_CONFIG_PATH" =~ "/opt/homebrew/lib/pkgconfig" ]]; then
        export PKG_CONFIG_PATH="/opt/homebrew/lib/pkgconfig:${PKG_CONFIG_PATH:-}"
    fi

    mkdir -p "$ZEROBREW_BIN"
    install -Dm755 target/debug/zb "$ZEROBREW_BIN/zb"
    echo "Installed zb to $ZEROBREW_BIN/zb"

    $ZEROBREW_BIN/zb init

[script]
uninstall:
    ZEROBREW_DIR="${ZEROBREW_DIR:-$HOME/.zerobrew}"
    ZEROBREW_BIN="${ZEROBREW_BIN:-$HOME/.local/bin}"
    ZEROBREW_OPT="${ZEROBREW_OPT:-/opt/zerobrew}"

    ZEROBREW_INSTALLED_BIN="${ZEROBREW_BIN%/}/zb"

    if command -v doas &>/dev/null; then
        SUDO="doas"
    elif command -v sudo &>/dev/null; then
        SUDO="sudo"
    else
        echo "ERROR: Neither sudo nor doas found" >&2
        exit 1
    fi

    shell_configs=(
        "${ZDOTDIR:-$HOME}/.zshenv"
        "${ZDOTDIR:-$HOME}/.zshrc"
        "$HOME/.bashrc"
        "$HOME/.bash_profile"
        "$HOME/.profile"
    )

    # Check which configs have zerobrew entries
    configs_to_clean=()
    for config in "${shell_configs[@]}"; do
        if [[ -f "$config" ]] && grep -q "^# zerobrew$" "$config" 2>/dev/null; then
            configs_to_clean+=("$config")
        fi
    done

    echo "Running this will remove:"
    echo -en "\x1b[1;31m"
    echo -e  "\t$ZEROBREW_INSTALLED_BIN"
    echo -e  "\t$ZEROBREW_DIR"
    echo -e  "\t$ZEROBREW_OPT"
    for config in "${configs_to_clean[@]}"; do
        echo -e "\tzerobrew entries in $config"
    done
    echo -en "\x1b[0m"
    read -rp "Continue? [y/N] " confirm

    [[ "$confirm" =~ ^[Yy]$ ]] || exit 0

    # Clean shell configuration files
    # Removes the entire zerobrew block added by install.sh:
    # - "# zerobrew" marker line
    # - ZEROBREW_DIR/ZEROBREW_BIN exports
    # - PKG_CONFIG_PATH export
    # - _zb_path_append function definition
    # - _zb_path_append calls
    for config in "${configs_to_clean[@]}"; do
        tmp_file=$(mktemp)
        sed -e '/^# zerobrew$/,/^}$/d' \
            -e '/_zb_path_append/d' \
            "$config" > "$tmp_file" 2>/dev/null || true
        # Remove consecutive blank lines (cleanup) and write back
        cat -s "$tmp_file" > "$config"
        rm "$tmp_file"
        echo -e "\x1b[1;32m✓\x1b[0m Cleaned $config"
    done

    [[ -f "$ZEROBREW_INSTALLED_BIN" ]] && rm -- "$ZEROBREW_INSTALLED_BIN"
    [[ -d "$ZEROBREW_DIR" ]] && rm -rf -- "$ZEROBREW_DIR"

    if [[ -d "$ZEROBREW_OPT" ]]; then
        $SUDO rm -r -- "$ZEROBREW_OPT"
    fi

    echo ""
    echo -e "\x1b[1;32m✓\x1b[0m zerobrew uninstalled successfully!"
    echo ""
    echo "Restart your terminal or run: exec \$SHELL"

# Reset zerobrew (clean shell configs, remove data, re-initialize)
[script]
reset:
    ZEROBREW_DIR="${ZEROBREW_DIR:-$HOME/.zerobrew}"
    ZEROBREW_OPT="${ZEROBREW_OPT:-/opt/zerobrew}"
    ZEROBREW_BIN="${ZEROBREW_BIN:-$HOME/.local/bin}"

    if command -v doas &>/dev/null; then
        SUDO="doas"
    elif command -v sudo &>/dev/null; then
        SUDO="sudo"
    else
        echo "ERROR: Neither sudo nor doas found" >&2
        exit 1
    fi

    shell_configs=(
        "${ZDOTDIR:-$HOME}/.zshenv"
        "${ZDOTDIR:-$HOME}/.zshrc"
        "$HOME/.bashrc"
        "$HOME/.bash_profile"
        "$HOME/.profile"
    )

    # Check which configs have zerobrew entries
    configs_to_clean=()
    for config in "${shell_configs[@]}"; do
        if [[ -f "$config" ]] && grep -q "^# zerobrew$" "$config" 2>/dev/null; then
            configs_to_clean+=("$config")
        fi
    done

    echo -e "\x1b[1;33mWarning:\x1b[0m This will reset zerobrew completely:"
    echo -en "\x1b[1;31m"
    echo -e  "\t$ZEROBREW_DIR"
    echo -e  "\t$ZEROBREW_OPT"
    for config in "${configs_to_clean[@]}"; do
        echo -e "\tzerobrew entries in $config"
    done
    echo -en "\x1b[0m"
    read -rp "Continue? [y/N] " confirm

    [[ "$confirm" =~ ^[Yy]$ ]] || exit 0

    # Clean shell configuration files
    # Removes the entire zerobrew block added by install.sh
    for config in "${configs_to_clean[@]}"; do
        tmp_file=$(mktemp)
        sed -e '/^# zerobrew$/,/^}$/d' \
            -e '/_zb_path_append/d' \
            "$config" > "$tmp_file" 2>/dev/null || true
        cat -s "$tmp_file" > "$config"
        rm "$tmp_file"
        echo -e "\x1b[1;32m✓\x1b[0m Cleaned $config"
    done

    # Remove directories
    [[ -d "$ZEROBREW_DIR" ]] && rm -rf -- "$ZEROBREW_DIR" && echo -e "\x1b[1;32m✓\x1b[0m Removed $ZEROBREW_DIR"

    if [[ -d "$ZEROBREW_OPT" ]]; then
        $SUDO rm -rf -- "$ZEROBREW_OPT" && echo -e "\x1b[1;32m✓\x1b[0m Removed $ZEROBREW_OPT"
    fi

    # Re-initialize
    echo ""
    echo -e "\x1b[1;36m==>\x1b[0m Re-initializing zerobrew..."
    "$ZEROBREW_BIN/zb" init

    echo ""
    echo -e "\x1b[1;32m✓\x1b[0m Reset complete!"

[script]
fmt:
    if command -v rustup &>/dev/null && rustup toolchain list | grep -q nightly; then
        cargo +nightly fmt --all -- --check
    else
        echo -e "\x1b[1;33mNote:\x1b[0m Using stable rustfmt (nightly not available)"
        cargo fmt --all -- --check
    fi

lint:
    cargo clippy --workspace -- -D warnings

test:
    cargo test --workspace
