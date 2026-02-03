set export
set dotenv-load
set script-interpreter := ['bash', '-euo', 'pipefail']

ZEROBREW_ROOT := if env('ZEROBREW_ROOT', '') != '' {
    env('ZEROBREW_ROOT')
} else if path_exists('/opt/zerobrew') == 'true' {
    '/opt/zerobrew'
} else if os() == 'macos' {
    '/opt/zerobrew'
} else {
    env('XDG_DATA_HOME', env('HOME', '~') / '.local' / 'share' ) / 'zerobrew'
}
ZEROBREW_DIR := env('ZEROBREW_DIR', env('HOME', '~') / '.zerobrew')
ZEROBREW_BIN := env('ZEROBREW_BIN', env('HOME', '~') / '.local' / 'bin')
ZEROBREW_PREFIX := env('ZEROBREW_PREFIX', ZEROBREW_ROOT / 'prefix')
ZEROBREW_INSTALLED_BIN := ZEROBREW_BIN / 'zb'

SUDO := if require('doas') != '' {
    'doas'
} else {
    require('sudo')
}

alias b := build
alias i := install
alias t := test
alias l := lint
alias f := fmt

[doc('List available recipes')]
default:
    @just --list --unsorted

[doc('Build the zb binary')]
[group('build')]
build: fmt lint
    cargo build --bin zb --bin zbx

[doc('Install zb to $ZEROBREW_BIN')]
[group('install')]
[script]
install: build
    if [[ -d "$ZEROBREW_PREFIX/lib/pkgconfig" ]]; then
        export PKG_CONFIG_PATH="$ZEROBREW_PREFIX/lib/pkgconfig:${PKG_CONFIG_PATH:-}"
    fi
    if [[ -d '/opt/homebrew/lib/pkgconfig' ]] && [[ ! "$PKG_CONFIG_PATH" =~ '/opt/homebrew/lib/pkgconfig' ]]; then
        export PKG_CONFIG_PATH="/opt/homebrew/lib/pkgconfig:${PKG_CONFIG_PATH:-}"
    fi

    mkdir -p "$ZEROBREW_BIN"
    install -Dm755 target/debug/zb "$ZEROBREW_BIN/zb"
    install -Dm755 target/debug/zbx "$ZEROBREW_BIN/zbx"
    echo "Installed zb to $ZEROBREW_BIN/zb"
    echo "Installed zbx to $ZEROBREW_BIN/zbx"

    "$ZEROBREW_BIN/zb" init

[private]
[script]
_get_zerobrew_configs:
    shell_configs=(
        "${ZDOTDIR:-$HOME}/.zshenv"
        "${ZDOTDIR:-$HOME}/.zshrc"
        "$HOME/.bashrc"
        "$HOME/.bash_profile"
        "$HOME/.profile"
    )

    for config in "${shell_configs[@]}"; do
        if [[ -f "$config" ]] && grep -q '^# zerobrew$' "$config" 2>/dev/null; then
            echo "$config"
        fi
    done

[private]
[script]
_clean_shell_config config:
    tmp_file=$(mktemp)
    sed -e '/^# zerobrew$/,/^}$/d' \
        -e '/_zb_path_append/d' \
        "$config" > "$tmp_file" 2>/dev/null || true
    cat -s "$tmp_file" > "$config"
    rm "$tmp_file"
    echo -e '{{BOLD}}{{GREEN}}✓{{NORMAL}} Cleaned '"$config"''

[private]
[script]
_confirm msg:
    read -rp "{{msg}} [y/N] " confirm
    if [[ "$confirm" =~ ^[Yy]$ ]]; then
        exit 0
    else
        exit 1
    fi

[doc('Uninstall zb and remove all data')]
[group('install')]
[script]
uninstall:
    mapfile -t configs_to_clean < <(just _get_zerobrew_configs)

    echo 'Running this will remove:'
    echo -en '{{BOLD}}{{RED}}'
    echo -e  "\t$ZEROBREW_INSTALLED_BIN"
    echo -e  "\t$ZEROBREW_DIR"
    echo -e  "\t$ZEROBREW_ROOT"
    for config in "${configs_to_clean[@]}"; do
        echo -e "\tzerobrew entries in $config"
    done
    echo -en '{{NORMAL}}'

    just _confirm "Continue?" || exit 0

    # Clean shell configuration files
    for config in "${configs_to_clean[@]}"; do
        just _clean_shell_config "$config"
    done

    [[ -f "$ZEROBREW_INSTALLED_BIN" ]] && rm -- "$ZEROBREW_INSTALLED_BIN"
    [[ -d "$ZEROBREW_DIR" ]] && rm -rf -- "$ZEROBREW_DIR"

    if [[ -d "$ZEROBREW_ROOT" ]]; then
        $SUDO rm -r -- "$ZEROBREW_ROOT"
    fi

    echo ''
    echo -e '{{BOLD}}{{GREEN}}✓{{NORMAL}} zerobrew uninstalled successfully!'
    echo ''
    echo 'Restart your terminal or run: exec $SHELL'

[doc('Reset zerobrew completely (removes data and re-initializes)')]
[group('install')]
[script]
reset:
    mapfile -t configs_to_clean < <(just _get_zerobrew_configs)

    echo -e '{{BOLD}}{{YELLOW}}Warning:{{NORMAL}} This will reset zerobrew completely:'
    echo -en '{{BOLD}}{{RED}}'
    echo -e  "\t$ZEROBREW_DIR"
    echo -e  "\t$ZEROBREW_ROOT"
    for config in "${configs_to_clean[@]}"; do
        echo -e "\tzerobrew entries in $config"
    done
    echo -en '{{NORMAL}}'

    just _confirm "Continue?" || exit 0

    # Clean shell configuration files
    for config in "${configs_to_clean[@]}"; do
        just _clean_shell_config "$config"
    done

    [[ -d "$ZEROBREW_DIR" ]] && rm -rf -- "$ZEROBREW_DIR" && echo -e '{{BOLD}}{{GREEN}}✓{{NORMAL}} Removed '"$ZEROBREW_DIR"''

    if [[ -d "$ZEROBREW_ROOT" ]]; then
        $SUDO rm -rf -- "$ZEROBREW_ROOT" && echo -e '{{BOLD}}{{GREEN}}✓{{NORMAL}} Removed '"$ZEROBREW_ROOT"''
    fi

    echo ''
    echo -e '{{BOLD}}{{CYAN}}==>{{NORMAL}} Re-initializing zerobrew...'

    if [[ -f "$ZEROBREW_INSTALLED_BIN" ]]; then
        "$ZEROBREW_INSTALLED_BIN" init
        echo ''
        echo -e '{{BOLD}}{{GREEN}}✓{{NORMAL}} Reset complete!'
    else
        echo -e '{{BOLD}}{{YELLOW}}Note:{{NORMAL}} zb binary not found at $ZEROBREW_INSTALLED_BIN'
        echo -e '{{BOLD}}{{YELLOW}}Note:{{NORMAL}} Run {{BOLD}}just install{{NORMAL}} first to install zerobrew'
    fi

[doc('Format code with rustfmt')]
[group('lint')]
[script]
fmt:
    if command -v rustup &>/dev/null && rustup toolchain list | grep -q nightly; then
        cargo +nightly fmt --all -- --check
    else
        echo -e '{{BOLD}}{{YELLOW}}Note:{{NORMAL}} Using stable rustfmt (nightly not available)'
        cargo fmt --all -- --check
    fi

[doc('Run Clippy linter')]
[group('lint')]
lint:
    cargo clippy --workspace -- -D warnings

[doc('Run all tests')]
[group('test')]
test:
    cargo test --workspace -- --include-ignored
