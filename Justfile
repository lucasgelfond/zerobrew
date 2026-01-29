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

    found=0
    zerobrew_pattern="zerobrew|\.local/bin.*PATH|/opt/zerobrew"
    shell_configs=(
        "${ZDOTDIR:-$HOME}/.zshenv"
        "${ZDOTDIR:-$HOME}/.zshrc"
        "$HOME/.bashrc"
        "$HOME/.bash_profile"
        "$HOME/.profile"
    )

    for config in "${shell_configs[@]}"; do
        if [[ -f "$config" ]] && grep -qE "$zerobrew_pattern" "$config" 2>/dev/null; then
            echo -e "\x1b[1;33mNote:\x1b[0m Found zerobrew in \$PATH: $config\x1b[0m"
            echo ""
            found=1
        fi
    done

    echo "Running this will remove:"
    echo -en "\x1b[1;31m"
    echo -e  "\t$ZEROBREW_INSTALLED_BIN"
    echo -e  "\t$ZEROBREW_DIR"
    echo -e  "\t$ZEROBREW_OPT"
    echo -en "\x1b[0m"
    read -rp "Continue? [y/N] " confirm

    [[ "$confirm" =~ ^[Yy]$ ]] || exit 0

    [[ -f "$ZEROBREW_INSTALLED_BIN" ]] && rm -- "$ZEROBREW_INSTALLED_BIN"
    [[ -d "$ZEROBREW_DIR" ]] && rm -rf -- "$ZEROBREW_DIR"

    if [[ -d "$ZEROBREW_OPT" ]]; then
        $SUDO rm -r -- "$ZEROBREW_OPT"
    fi

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
