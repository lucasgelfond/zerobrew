#!/usr/bin/env bash
set -euo pipefail

# Restore cursor on exit
restore_cursor() {
    printf '\033[?25h'
}
trap restore_cursor EXIT

ZEROBREW_REPO="https://github.com/lucasgelfond/zerobrew.git"
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

# Allow custom prefix, default to $ZEROBREW_ROOT/prefix
: ${ZEROBREW_PREFIX:=$ZEROBREW_ROOT/prefix}

export ZEROBREW_ROOT
export ZEROBREW_PREFIX

# Prevent running with sudo - the script handles its own privilege escalation
if [[ $EUID -eq 0 ]]; then
    error_exit "Do not run this script with sudo or as root. The installer will automatically request privileges when needed."
fi

no_modify_path=false
binary_paths=()

MUTED=$'\033[0;2m'
RED=$'\033[0;31m'
ORANGE=$'\033[38;5;214m'
GREEN=$'\033[0;32m'
NC=$'\033[0m'

usage() {
    printf "zero%bbrew%b Installer\n" "$ORANGE" "$NC"
    printf "\n"
    printf "Usage: install.sh %b[options]%b\n" "$MUTED" "$NC"
    printf "\n"
    printf "Options:\n"
    printf "    -h, --help               %bDisplay this help message%b\n" "$MUTED" "$NC"
    printf "    -b, --binary <path>...   %bInstalls binaries (zb, zbx) to \$ZEROBREW_BIN%b\n" "$MUTED" "$NC"
    printf "        --no-modify-path     %bDon't modify shell config files (.zshrc, .bashrc, etc.)%b\n" "$MUTED" "$NC"
    printf "\n"
    printf "Examples:%b\n" "$MUTED"
    printf "    ./install.sh --no-modify-path\n"
    printf "    ./install.sh -b /path/to/zb\n"
    printf "    ./install.sh -b /path/to/zb /path/to/zbx%b\n" "$NC"
}

spinner() {
    local msg="$1"
    local pid=$2
    local spin='|/-\'
    local i=0
    local exit_code=0

    printf '\033[?25l'

    while kill -0 $pid 2>/dev/null; do
        i=$(((i + 1) % 4))
        printf "\r%b[%s]%b %b" "$ORANGE" "${spin:$i:1}" "$NC" "$msg"
        sleep 0.1
    done

    wait $pid 2>/dev/null && exit_code=0 || exit_code=$?

    printf "\r\033[K"
    printf '\033[?25h'

    return $exit_code
}

completed() {
    printf "%b[✓]%b %b\n" "$GREEN" "$NC" "$1"
}

error_exit() {
    local msg="$1"
    local exit_code="${2:-1}"
    printf "\r\033[K"
    printf '\033[?25h'
    printf "%b[✗]%b %b\n" "$RED" "$NC" "$msg" >&2
    exit $exit_code
}

check_command() {
    local cmd="$1"
    local install_hint="${2:-}"

    if ! command -v "$cmd" >/dev/null 2>&1; then
        local msg="Required command '$cmd' not found"
        if [[ -n "$install_hint" ]]; then
            msg="$msg. Hint: $install_hint"
        fi
        error_exit "$msg"
    fi
}

install_bin() {
    local target_dir="$1"
    shift
    local paths_to_install=("$@")

    if ! mkdir -p "$target_dir"; then
        error_exit "Failed to create directory: $target_dir"
    fi

    for binary_path in "${paths_to_install[@]}"; do
        if [[ ! -f "$binary_path" ]]; then
            error_exit "Binary not found at ${binary_path}"
        fi

        local binary_name
        binary_name=$(basename "$binary_path")

        if ! install -Dm755 "$binary_path" "$target_dir/$binary_name"; then
            error_exit "Failed to copy $binary_name to $target_dir"
        fi

        completed "Installed ${ORANGE}$binary_name${NC} to $target_dir"
    done
}

zb_init() {
    local zb_path="$1"
    local no_modify="$2"
    local init_args=()

    if [[ "$no_modify" == "true" ]]; then
        init_args+=("--no-modify-path")
    fi

    "$zb_path" init "${init_args[@]}" >/dev/null 2>&1 || error_exit "Failed to initialize zerobrew"
}

print_logo() {
    printf "\n"
    printf "%b▄▄▄▄▄ ▄▄▄▄▄ ▄▄▄▄   ▄▄▄ %b ▄▄▄▄  ▄▄▄▄  ▄▄▄▄▄ ▄▄   ▄▄\n" "$NC" "$ORANGE"
    printf "%b  ▄█▀ ██▄▄  ██▄█▄ ██▀██%b ██▄██ ██▄█▄ ██▄▄  ██ ▄ ██\n" "$NC" "$ORANGE"
    printf "%b▄██▄▄ ██▄▄▄ ██ ██ ▀███▀%b ██▄█▀ ██ ██ ██▄▄▄  ▀█▀█▀ \n" "$NC" "$ORANGE"
    printf "\n"

    printf "%bStart installing %bPackages%b with %bzerobrew%b:\n\n" "$MUTED" "$NC" "$MUTED" "$ORANGE" "$NC"
    printf "  zb install %bffmpeg%b    # Install a Package%b\n" "$ORANGE" "$MUTED" "$NC"
    printf "  zbx %byetris%b           # Single-time Run\n\n" "$ORANGE" "$MUTED" "$NC"
    printf "%bFor more information visit %bhttps://zerobrew.rs/docs\n\n" "$MUTED" "$NC"
}

while [[ $# -gt 0 ]]; do
    case "$1" in
    -h | --help)
        usage
        exit 0
        ;;
    --no-modify-path)
        no_modify_path=true
        shift
        ;;
    -b | --binary)
        if [[ -n "${2:-}" ]]; then
            binary_paths+=("$2")
            shift 2
            if [[ -n "${1:-}" && "${1:0:1}" != "-" ]]; then
                binary_paths+=("$1")
                shift
            fi
        else
            error_exit "--binary requires a path argument"
        fi
        ;;
    *)
        error_exit "Unknown option '%s'" "$1"
        ;;
    esac
done

# Skip all if binary path is provided
if [[ ${#binary_paths[@]} -gt 0 ]]; then
    install_bin "$ZEROBREW_BIN" "${binary_paths[@]}"

    zb_init "$ZEROBREW_BIN/zb" "$no_modify_path"

    print_logo
    completed "Installation complete"
    exit 0
fi

# Check for required commands
check_command "curl" "Install curl using your package manager (e.g., 'brew install curl' on macOS)"
check_command "git" "Install git using your package manager (e.g., 'brew install git' on macOS)"
check_command "mkdir" "Your system should have mkdir installed by default"
check_command "cp" "Your system should have cp installed by default"
check_command "chmod" "Your system should have chmod installed by default"

# Check for Rust/Cargo
if ! command -v cargo >/dev/null 2>&1; then
    (
        curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
    ) &
    if ! spinner "Installing ${ORANGE}Rust toolchain${NC}" $!; then
        error_exit "Failed to install Rust toolchain. Check your network connection and try again."
    fi
    # shellcheck source=/dev/null
    source "$HOME/.cargo/env"
    completed "${ORANGE}Rust toolchain${NC} installed"
fi

# Ensure cargo is available
if ! command -v cargo >/dev/null 2>&1; then
    error_exit "Cargo not found after installing Rust. Try restarting your terminal or running: source ~/.cargo/env"
fi

# Clone or update repo
if [[ -d "$ZEROBREW_DIR" ]]; then
    (
        cd "$ZEROBREW_DIR" || exit 1
        if ! git fetch --depth=1 origin main >/dev/null 2>&1; then
            printf "Failed to fetch updates\n" >&2
            exit 1
        fi
        if ! git reset --hard origin/main >/dev/null 2>&1; then
            printf "Failed to reset to origin/main\n" >&2
            exit 1
        fi
    ) &
    if ! spinner "Updating ${ORANGE}zerobrew${NC} repository" $!; then
        error_exit "Failed to update zerobrew repository. Check your network connection and permissions."
    fi
    completed "Updated ${ORANGE}zerobrew${NC} repository"
    cd "$ZEROBREW_DIR" || error_exit "Failed to enter directory: $ZEROBREW_DIR"
else
    (
        if ! git clone --depth 1 "$ZEROBREW_REPO" "$ZEROBREW_DIR" >/dev/null 2>&1; then
            printf "Failed to clone repository\n" >&2
            exit 1
        fi
    ) &
    if ! spinner "Cloning ${ORANGE}zerobrew${NC} repository" $!; then
        error_exit "Failed to clone zerobrew repository. Check your network connection and that the repository exists."
    fi
    completed "Cloned ${ORANGE}zerobrew${NC} repository"
    cd "$ZEROBREW_DIR" || error_exit "Failed to enter directory: $ZEROBREW_DIR"
fi

# Build
if [[ -d "$ZEROBREW_PREFIX/lib/pkgconfig" ]]; then
    export PKG_CONFIG_PATH="$ZEROBREW_PREFIX/lib/pkgconfig:${PKG_CONFIG_PATH:-}"
fi
if [[ -d "/opt/homebrew/lib/pkgconfig" ]] && [[ ! "$PKG_CONFIG_PATH" =~ "/opt/homebrew/lib/pkgconfig" ]]; then
    export PKG_CONFIG_PATH="/opt/homebrew/lib/pkgconfig:${PKG_CONFIG_PATH:-}"
fi

(
    if ! cargo build --release --bin zb --bin zbx >/dev/null 2>&1; then
        error_exit "Build failed. Run 'cargo build --bin zb --bin zbx' to see details."
    fi
) &
if ! spinner "Building ${ORANGE}zerobrew${NC}" $!; then
    error_exit "Failed to build zerobrew. Ensure Rust and dependencies are properly installed."
fi
completed "Built ${ORANGE}zerobrew${NC}"

if [[ ! -f "target/release/zb" ]]; then
    error_exit "Build succeeded but zb binary not found at target/release/zb"
fi

if [[ ! -f "target/release/zbx" ]]; then
    error_exit "Build succeeded but zbx binary not found at target/release/zbx"
fi

install_bin "$ZEROBREW_BIN" target/release/zb target/release/zbx

# Verify the binary works
if ! "$ZEROBREW_BIN/zb" --version >/dev/null 2>&1; then
    error_exit "Installation succeeded but binary does not execute properly"
fi

# Add zb to PATH for current session if not already present
if [[ ":$PATH:" != *":$ZEROBREW_BIN:"* ]]; then
    export PATH="$ZEROBREW_BIN:$PATH"
fi

zb_init "$ZEROBREW_BIN/zb" "$no_modify_path"

print_logo
