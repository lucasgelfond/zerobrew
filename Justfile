set export
set dotenv-load
set unstable
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
ZEROBREW_PREFIX := if env('ZEROBREW_PREFIX', '') != '' {
    env('ZEROBREW_PREFIX')
} else if os() == 'macos' {
    ZEROBREW_ROOT
} else {
    ZEROBREW_ROOT / 'prefix'
}
ZEROBREW_INSTALLED_BIN := ZEROBREW_BIN / 'zb'

SUDO := if which('doas') != '' {
    'doas'
} else {
    require('sudo')
}

# Package lists for benchmarks
BENCH_PACKAGES := 'ca-certificates openssl@3 xz sqlite readline icu4c@78 python@3.14 awscli node harfbuzz ncurses gh pcre2 libpng zstd glib lz4 gettext libngtcp2 libnghttp3 pkgconf libunistring mpdecimal brotli jpeg-turbo xorgproto ffmpeg cmake libnghttp2 go uv gmp libtiff fontconfig python@3.13 git little-cms2 dav1d openexr c-ares tesseract p11-kit imagemagick zlib libx11 freetype protobuf gnupg openjph libtasn1 ruby gnutls expat libsodium simdjson gemini-cli libarchive pyenv pixman curl opus unbound cairo pango leptonica libxcb jpeg-xl coreutils certifi krb5 docker libheif webp libxext libxau gcc bzip2 libxdmcp abseil xcbeautify libuv giflib utf8proc libxrender m4 graphite2 openjdk uvwasi libffi libdeflate llvm aom lzo libevent libgpg-error libidn2 berkeley-db@5 deno libedit oniguruma'

BENCH_QUICK_PACKAGES := 'jq tree htop bat fd ripgrep fzf wget curl git tmux zoxide openssl@3 sqlite readline pcre2 zstd lz4 node go ruby gh'

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

[doc('Run benchmark comparing zerobrew vs homebrew')]
[group('benchmark')]
[positional-arguments]
[script]
bench *args:
    # Package lists defined in Justfile variables
    read -ra PACKAGES <<< "{{BENCH_PACKAGES}}"
    read -ra QUICK_PACKAGES <<< "{{BENCH_QUICK_PACKAGES}}"

    FORMAT=""
    OUTPUT=""
    COUNT=""
    QUICK=true
    FULL=false
    FULL_OUTPUT_DIR=""
    NO_COLOR=false
    LOG_FILE=""
    DRY_RUN=false

    need_arg() { [[ -n "$2" && "$2" != --* ]] || { echo "Error: $1 requires a value" >&2; exit 1; }; }

    while [[ $# -gt 0 ]]; do
        case $1 in
            --format)  need_arg "$1" "$2"; FORMAT="$2"; shift 2 ;;
            -c|--count)   need_arg "$1" "$2"; COUNT="$2"; shift 2 ;;
            --quick)   QUICK=true; FULL=false; shift ;;
            --full)    FULL=true; QUICK=false;
                       # Check if next arg is a directory path (not a flag)
                       if [[ $# -gt 1 && "$2" != -* ]]; then
                           FULL_OUTPUT_DIR="$2"
                           shift 2
                       else
                           shift
                       fi ;;
            --no-color) NO_COLOR=true; shift ;;
            --log)     need_arg "$1" "$2"; LOG_FILE="$2"; shift 2 ;;
            -o|--output) need_arg "$1" "$2"; OUTPUT="$2"; shift 2 ;;
            --dry-run) DRY_RUN=true; shift ;;
            -h|--help)
                echo "Usage: just bench [options]"
                echo ""
                echo "Options:"
                echo "  --quick              Test all quick packages (default, 22 packages)"
                echo "  --full [DIR]         Test all 100 top Homebrew packages"
                echo "                       Optionally specify DIR to output all formats to directory"
                echo "  -c, --count N        Test first N packages from selected list"
                echo "                       (quick packages by default, or from --full list)"
                echo "  --format FORMAT      Output format: text (default), json, csv, or html"
                echo "  -o, --output FILE    Write output to file instead of stdout (format inferred from extension)"
                echo "  --no-color           Disable colored output"
                echo "  --log FILE           Write install command logs to file"
                echo "  --dry-run            Show what would be tested without running benchmarks"
                echo "  -h, --help           Show this help message"
                exit 0 ;;
            *) echo "Unknown option: $1" >&2; exit 1 ;;
        esac
    done

    # Infer format from output file extension if not explicitly set
    if [[ -z "$FORMAT" && -n "$OUTPUT" ]]; then
        case "$OUTPUT" in
            *.json) FORMAT="json" ;;
            *.csv)  FORMAT="csv" ;;
            *.html) FORMAT="html" ;;
            *)      FORMAT="text" ;;
        esac
    elif [[ -z "$FORMAT" ]]; then
        FORMAT="text"
    fi

    [[ "$FORMAT" =~ ^(text|json|csv|html)$ ]] || { echo "Error: format must be text, json, csv, or html" >&2; exit 1; }

    # Pre-run validation
    missing=()
    command -v brew &>/dev/null || missing+=("brew")
    command -v zb &>/dev/null || missing+=("zb")
    command -v python3 &>/dev/null || missing+=("python3")
    command -v bc &>/dev/null || missing+=("bc")
    command -v sed &>/dev/null || missing+=("sed")

    if [[ ${#missing[@]} -gt 0 ]]; then
        echo "Error: Missing required commands: ${missing[*]}" >&2
        echo "Please install them before running benchmarks." >&2
        exit 1
    fi

    export HOMEBREW_NO_AUTO_UPDATE=1
    export HOMEBREW_NO_ANALYTICS=1
    export HOMEBREW_NO_ENV_HINTS=1

    brew_prefix=$(brew --prefix)
    if [[ ! -w "$brew_prefix" ]]; then
        echo "Error: Homebrew prefix is not writable: $brew_prefix" >&2
        echo "This will cause brew to prompt for sudo during benchmarks." >&2
        echo "Fix: sudo chown -R \"$(whoami)\" \"$brew_prefix\"" >&2
        exit 1
    fi

    # Check if zerobrew directories have files not owned by current user
    # This would cause zb reset to prompt for sudo during benchmarks
    current_user=$(whoami)
    needs_chown=false

    for dir in "$ZEROBREW_ROOT" "$ZEROBREW_PREFIX"; do
        if [[ -d "$dir" ]]; then
            # Find any files/dirs not owned by current user (limit to 1 for speed)
            not_owned=$(find "$dir" ! -user "$current_user" -print -quit 2>/dev/null)
            if [[ -n "$not_owned" ]]; then
                needs_chown=true
                break
            fi
        fi
    done

    if [[ "$needs_chown" == "true" ]]; then
        echo -e "${YELLOW}==> Some files in zerobrew directories are not owned by you.${NORMAL}" >&2
        echo -e "${YELLOW}    This will cause password prompts during benchmarks.${NORMAL}" >&2
        echo -e "${YELLOW}    Fixing ownership now (requires sudo once)...${NORMAL}" >&2
        if [[ -d "$ZEROBREW_ROOT" ]]; then
            {{SUDO}} chown -R "$current_user" "$ZEROBREW_ROOT" || { echo "Error: Failed to fix ownership of $ZEROBREW_ROOT" >&2; exit 1; }
        fi
        if [[ -d "$ZEROBREW_PREFIX" && "$ZEROBREW_PREFIX" != "$ZEROBREW_ROOT"* ]]; then
            {{SUDO}} chown -R "$current_user" "$ZEROBREW_PREFIX" || { echo "Error: Failed to fix ownership of $ZEROBREW_PREFIX" >&2; exit 1; }
        fi
        echo -e "${GREEN}    Ownership fixed!${NORMAL}" >&2
    fi

    # Determine which packages to test
    # Default: all quick packages (22)
    # --quick: all quick packages
    # --full: all 100 packages
    # --count N: limit currently selected list to N
    if [[ "$FULL" == "true" ]]; then
        # --full: use all 100 packages
        PACKAGES=("${PACKAGES[@]}")
    else
        # Default or --quick: use all quick packages
        PACKAGES=("${QUICK_PACKAGES[@]}")
    fi

    # Apply --count limit if specified
    if [[ -n "$COUNT" ]]; then
        PACKAGES=("${PACKAGES[@]:0:$COUNT}")
    fi

    if [[ "$NO_COLOR" == "true" ]]; then
        RED="" GREEN="" YELLOW="" BLUE="" CYAN="" NORMAL="" BOLD=""
    else
        RED="{{RED}}" GREEN="{{GREEN}}" YELLOW="{{YELLOW}}"
        BLUE="{{BLUE}}" CYAN="{{CYAN}}" NORMAL="{{NORMAL}}" BOLD="{{BOLD}}"
    fi

    if [[ -n "$LOG_FILE" ]]; then
        : > "$LOG_FILE"
        echo -e "${BLUE}Debug logging to: $LOG_FILE${NORMAL}" >&2
    fi

    log_msg() {
        [[ -n "$LOG_FILE" ]] || return 0
        printf "[%s] %s\n" "$(date -u +'%Y-%m-%dT%H:%M:%SZ')" "$*" >> "$LOG_FILE"
    }

    format_duration() {
        python3 -c "ms=int('$1'); print(f'{ms/1000:.2f}s' if ms>=1000 else f'{ms}ms')"
    }

    log_msg "bench start: format=$FORMAT full=$FULL quick=$QUICK count=${COUNT:-all} packages=${#PACKAGES[@]}"
    if [[ -n "$FULL_OUTPUT_DIR" ]]; then
        log_msg "output dir: $FULL_OUTPUT_DIR"
    elif [[ -n "$OUTPUT" ]]; then
        log_msg "output file: $OUTPUT"
    fi

    if [[ "$DRY_RUN" == "true" ]]; then
        echo -e "${CYAN}=== Dry Run ===${NORMAL}" >&2
        echo "Would test ${#PACKAGES[@]} packages:" >&2
        for pkg in "${PACKAGES[@]}"; do
            echo "  - $pkg" >&2
        done
        echo "" >&2
        if [[ -n "$FULL_OUTPUT_DIR" ]]; then
            echo "Output directory: $FULL_OUTPUT_DIR" >&2
            echo "  Will create benchmark.{txt,json,csv,html}" >&2
        else
            echo "Format: $FORMAT" >&2
            [[ -n "$OUTPUT" ]] && echo "Output file: $OUTPUT" >&2
        fi
        [[ -n "$LOG_FILE" ]] && echo "Log file: $LOG_FILE" >&2
        echo "" >&2
        echo "Each package would be tested with:" >&2
        echo "  1. brew install <package>" >&2
        echo "  2. zb install <package> (cold cache)" >&2
        echo "  3. zb install <package> (warm cache)" >&2
        exit 0
    fi

    # Portable timing using python3 (works on macOS + Linux)
    get_time() { python3 -c "import time; print(time.time())"; }
    elapsed_ms() { python3 -c "print(int((float('$2') - float('$1')) * 1000))"; }

    # Safer command execution using argv array
    # Usage: run_timed_install "label" cmd arg1 arg2 ...
    run_timed_install() {
        local label="$1"
        shift
        echo -e "  ${YELLOW}-> $label...${NORMAL}" >&2
        local start=$(get_time)
        local status=0
        if [[ -n "$LOG_FILE" ]]; then
            log_msg "START $label: $*"
            "$@" >> "$LOG_FILE" 2>&1
            status=$?
        else
            "$@" > /dev/null 2>&1
            status=$?
        fi

        if [[ $status -eq 0 ]]; then
            local elapsed
            elapsed=$(elapsed_ms "$start" "$(get_time)")
            log_msg "END $label: ${elapsed}ms"
            echo "$elapsed"
            return 0
        fi

        log_msg "FAIL $label (exit $status)"
        return 1
    }

    declare -a NAMES=() BREW_TIMES=() ZB_COLD_TIMES=() ZB_WARM_TIMES=() SPEEDUPS_COLD=() SPEEDUPS_WARM=() FAILED_NAMES=() FAILED_REASONS=()
    PASSED=0
    FAILED=0

    for i in "${!PACKAGES[@]}"; do
        pkg="${PACKAGES[$i]}"
        idx=$((i + 1))
        echo -e "${CYAN}[$idx/${#PACKAGES[@]}] Testing: $pkg${NORMAL}" >&2
        log_msg "package start: $pkg ($idx/${#PACKAGES[@]})"

        brew uninstall --ignore-dependencies "$pkg" &>/dev/null || true
        zb uninstall "$pkg" &>/dev/null || true
        zb reset -y &>/dev/null || true

        if BREW_MS=$(run_timed_install "Homebrew" brew install "$pkg"); then
            echo -e "    ${GREEN}OK: $(format_duration "$BREW_MS")${NORMAL}" >&2
        else
            echo -e "    ${RED}FAILED${NORMAL}" >&2
            FAILED_NAMES+=("$pkg")
            FAILED_REASONS+=("brew install failed")
            ((FAILED++)) || true  # || true needed because ((0++)) returns exit 1
            log_msg "package fail: $pkg (brew install failed)"
            continue
        fi

        brew uninstall --ignore-dependencies "$pkg" &>/dev/null || true

        zb reset -y &>/dev/null || true
        if ZB_COLD_MS=$(run_timed_install "Zerobrew (cold)" zb install "$pkg"); then
            echo -e "    ${GREEN}OK: $(format_duration "$ZB_COLD_MS")${NORMAL}" >&2
        else
            echo -e "    ${RED}FAILED${NORMAL}" >&2
            FAILED_NAMES+=("$pkg")
            FAILED_REASONS+=("zb install failed (cold)")
            ((FAILED++)) || true  # || true needed because ((0++)) returns exit 1
            log_msg "package fail: $pkg (zb install failed cold)"
            continue
        fi

        zb uninstall "$pkg" &>/dev/null || true
        if ZB_WARM_MS=$(run_timed_install "Zerobrew (warm)" zb install "$pkg"); then
            echo -e "    ${GREEN}OK: $(format_duration "$ZB_WARM_MS")${NORMAL}" >&2
        else
            echo -e "    ${RED}FAILED${NORMAL}" >&2
            FAILED_NAMES+=("$pkg")
            FAILED_REASONS+=("zb install failed (warm)")
            ((FAILED++)) || true  # || true needed because ((0++)) returns exit 1
            log_msg "package fail: $pkg (zb install failed warm)"
            continue
        fi

        SPEEDUP_COLD=$( [[ $ZB_COLD_MS -gt 0 ]] && echo "scale=2; $BREW_MS / $ZB_COLD_MS" | bc -l | sed 's/^\./0./' || echo "0" )
        SPEEDUP_WARM=$( [[ $ZB_WARM_MS -gt 0 ]] && echo "scale=2; $BREW_MS / $ZB_WARM_MS" | bc -l | sed 's/^\./0./' || echo "0" )

        NAMES+=("$pkg")
        BREW_TIMES+=("$BREW_MS")
        ZB_COLD_TIMES+=("$ZB_COLD_MS")
        ZB_WARM_TIMES+=("$ZB_WARM_MS")
        SPEEDUPS_COLD+=("$SPEEDUP_COLD")
        SPEEDUPS_WARM+=("$SPEEDUP_WARM")
        ((PASSED++)) || true
        log_msg "package done: $pkg"

        zb uninstall "$pkg" &>/dev/null || true
        echo >&2
    done

    echo "Cleaning up..." >&2
    log_msg "cleanup start"
    for pkg in "${PACKAGES[@]}"; do
        brew uninstall --ignore-dependencies "$pkg" 2>/dev/null || true
        zb uninstall "$pkg" 2>/dev/null || true
    done
    zb uninstall 2>/dev/null || true

    TOTAL_BREW=0
    TOTAL_ZB_COLD=0
    TOTAL_ZB_WARM=0

    for i in "${!NAMES[@]}"; do
        TOTAL_BREW=$((TOTAL_BREW + BREW_TIMES[i]))
        TOTAL_ZB_COLD=$((TOTAL_ZB_COLD + ZB_COLD_TIMES[i]))
        TOTAL_ZB_WARM=$((TOTAL_ZB_WARM + ZB_WARM_TIMES[i]))
    done

    if [[ $PASSED -gt 0 ]]; then
        AVG_SPEEDUP_COLD=$( [[ $TOTAL_ZB_COLD -gt 0 ]] && echo "scale=2; $TOTAL_BREW / $TOTAL_ZB_COLD" | bc -l | sed 's/^\./0./' || echo "0" )
        AVG_SPEEDUP_WARM=$( [[ $TOTAL_ZB_WARM -gt 0 ]] && echo "scale=2; $TOTAL_BREW / $TOTAL_ZB_WARM" | bc -l | sed 's/^\./0./' || echo "0" )
    else
        AVG_SPEEDUP_COLD="0"
        AVG_SPEEDUP_WARM="0"
    fi
    log_msg "bench done: passed=$PASSED failed=$FAILED"

    output_text() {
        echo "=== Benchmark Summary ==="
        echo "Tested: ${#PACKAGES[@]} packages"
        echo "Passed: $PASSED"
        echo "Failed: $FAILED"
        echo ""
        echo "Performance:"
        echo "  Average speedup (cold): ${AVG_SPEEDUP_COLD}x"
        echo "  Average speedup (warm): ${AVG_SPEEDUP_WARM}x"
        echo ""
        echo "Results:"
        echo "Package             Homebrew      ZB Cold      ZB Warm   Speed (cold/warm)"
        echo "--------------------------------------------------------------------------"
        for i in "${!NAMES[@]}"; do
            printf "%-15s %12s %12s %12s %10sx / %sx\n" "${NAMES[i]}" "$(format_duration "${BREW_TIMES[i]}")" "$(format_duration "${ZB_COLD_TIMES[i]}")" "$(format_duration "${ZB_WARM_TIMES[i]}")" "${SPEEDUPS_COLD[i]}" "${SPEEDUPS_WARM[i]}"
        done
        echo
        if [[ $FAILED -gt 0 ]]; then
            echo "Failed Packages:"
            for i in "${!FAILED_NAMES[@]}"; do
                echo "  ${FAILED_NAMES[i]} - ${FAILED_REASONS[i]}"
            done
        fi
        echo "Done."
    }

    output_json() {
        printf '{"results":['
        first=1
        for i in "${!NAMES[@]}"; do
            [[ $first -eq 0 ]] && printf ","
            first=0
            printf '{"name":"%s","homebrew_ms":%s,"zerobrew_cold_ms":%s,"zerobrew_warm_ms":%s,"speedup_cold":%s,"speedup_warm":%s}' "${NAMES[i]}" "${BREW_TIMES[i]}" "${ZB_COLD_TIMES[i]}" "${ZB_WARM_TIMES[i]}" "${SPEEDUPS_COLD[i]}" "${SPEEDUPS_WARM[i]}"
        done
        printf '],"failures":['
        first=1
        for i in "${!FAILED_NAMES[@]}"; do
            [[ $first -eq 0 ]] && printf ","
            first=0
            printf '{"name":"%s","reason":"%s"}' "${FAILED_NAMES[i]}" "${FAILED_REASONS[i]}"
        done
        printf '],"summary":{"tested":%d,"passed":%d,"failed":%d,"avg_speedup_cold":%s,"avg_speedup_warm":%s}}\n' "${#PACKAGES[@]}" "$PASSED" "$FAILED" "$AVG_SPEEDUP_COLD" "$AVG_SPEEDUP_WARM"
    }

    output_csv() {
        echo "package,homebrew_ms,zerobrew_cold_ms,zerobrew_warm_ms,speedup_cold,speedup_warm"
        for i in "${!NAMES[@]}"; do
            echo "${NAMES[i]},${BREW_TIMES[i]},${ZB_COLD_TIMES[i]},${ZB_WARM_TIMES[i]},${SPEEDUPS_COLD[i]},${SPEEDUPS_WARM[i]}"
        done
    }

    output_html() {
        echo '<!DOCTYPE html>'
        echo '<html>'
        echo '<head>'
        echo '    <title>Zerobrew Benchmark Results</title>'
        echo '    <style>'
        echo '        body { font-family: -apple-system, BlinkMacSystemFont, '"'"'Segoe UI'"'"', Roboto, sans-serif; margin: 40px; background: #f5f5f5; }'
        echo '        .container { max-width: 1000px; margin: 0 auto; background: white; padding: 30px; border-radius: 8px; box-shadow: 0 2px 4px rgba(0,0,0,0.1); }'
        echo '        h1 { color: #333; border-bottom: 2px solid #0066cc; padding-bottom: 10px; }'
        echo '        .summary { display: grid; grid-template-columns: repeat(auto-fit, minmax(150px, 1fr)); gap: 20px; margin: 20px 0; }'
        echo '        .stat { background: #f8f9fa; padding: 20px; border-radius: 8px; text-align: center; }'
        echo '        .stat-value { font-size: 2em; font-weight: bold; color: #0066cc; }'
        echo '        .stat-label { color: #666; margin-top: 5px; }'
        echo '        table { width: 100%; border-collapse: collapse; margin: 20px 0; }'
        echo '        th, td { padding: 12px; text-align: left; border-bottom: 1px solid #ddd; }'
        echo '        th { background: #0066cc; color: white; }'
        echo '        tr:hover { background: #f5f5f5; }'
        echo '        .speedup { font-weight: bold; color: #28a745; }'
        echo '        .failed { color: #dc3545; }'
        echo '        .timestamp { color: #999; font-size: 0.9em; margin-top: 20px; }'
        echo '    </style>'
        echo '</head>'
        echo '<body>'
        echo '    <div class="container">'
        echo '        <h1>Zerobrew Benchmark Results</h1>'
        echo '        <div class="summary">'
        echo '            <div class="stat">'
        echo "                <div class=\"stat-value\">${#PACKAGES[@]}</div>"
        echo '                <div class="stat-label">Packages Tested</div>'
        echo '            </div>'
        echo '            <div class="stat">'
        echo "                <div class=\"stat-value\">$PASSED</div>"
        echo '                <div class="stat-label">Passed</div>'
        echo '            </div>'
        echo '            <div class="stat">'
        echo "                <div class=\"stat-value\">${AVG_SPEEDUP_COLD}x</div>"
        echo '                <div class="stat-label">Avg Cold Speedup</div>'
        echo '            </div>'
        echo '            <div class="stat">'
        echo "                <div class=\"stat-value\">${AVG_SPEEDUP_WARM}x</div>"
        echo '                <div class="stat-label">Avg Warm Speedup</div>'
        echo '            </div>'
        echo '        </div>'
        echo '        <h2>Results</h2>'
        echo '        <table>'
        echo '            <thead>'
        echo '                <tr>'
        echo '                    <th>Package</th>'
        echo '                    <th>Homebrew</th>'
        echo '                    <th>ZB Cold</th>'
        echo '                    <th>ZB Warm</th>'
        echo '                    <th>Speedup (cold/warm)</th>'
        echo '                </tr>'
        echo '            </thead>'
        echo '            <tbody>'
        for i in "${!NAMES[@]}"; do
            echo "                <tr>"
            echo "                    <td>${NAMES[$i]}</td>"
            echo "                    <td>$(format_duration "${BREW_TIMES[$i]}")</td>"
            echo "                    <td>$(format_duration "${ZB_COLD_TIMES[$i]}")</td>"
            echo "                    <td>$(format_duration "${ZB_WARM_TIMES[$i]}")</td>"
            echo "                    <td class=\"speedup\">${SPEEDUPS_COLD[$i]}x / ${SPEEDUPS_WARM[$i]}x</td>"
            echo "                </tr>"
        done
        echo '            </tbody>'
        echo '        </table>'
        if [[ $FAILED -gt 0 ]]; then
            echo '        <h2>Failed Packages</h2>'
            echo '        <ul>'
            for i in "${!FAILED_NAMES[@]}"; do
                echo "            <li class=\"failed\"><strong>${FAILED_NAMES[$i]}</strong>: ${FAILED_REASONS[$i]}</li>"
            done
            echo '        </ul>'
        fi
        TIMESTAMP=$(date)
        echo "        <div class=\"timestamp\">Generated: $TIMESTAMP</div>"
        echo '    </div>'
        echo '</body>'
        echo '</html>'
    }

    output_result() {
        case "$FORMAT" in
            text) output_text ;;
            json) output_json ;;
            csv)  output_csv ;;
            html) output_html ;;
        esac
    }

    if [[ -n "$FULL_OUTPUT_DIR" ]]; then
        # Output all formats to the specified directory
        mkdir -p "$FULL_OUTPUT_DIR"

        BASE_NAME="benchmark"

        echo -e "${CYAN}Writing all formats to: $FULL_OUTPUT_DIR${NORMAL}" >&2

        FORMAT="text" output_result > "$FULL_OUTPUT_DIR/${BASE_NAME}.txt"
        echo -e "${GREEN}  ✓ ${BASE_NAME}.txt${NORMAL}" >&2

        FORMAT="json" output_result > "$FULL_OUTPUT_DIR/${BASE_NAME}.json"
        echo -e "${GREEN}  ✓ ${BASE_NAME}.json${NORMAL}" >&2

        FORMAT="csv" output_result > "$FULL_OUTPUT_DIR/${BASE_NAME}.csv"
        echo -e "${GREEN}  ✓ ${BASE_NAME}.csv${NORMAL}" >&2

        FORMAT="html" output_result > "$FULL_OUTPUT_DIR/${BASE_NAME}.html"
        echo -e "${GREEN}  ✓ ${BASE_NAME}.html${NORMAL}" >&2

        echo -e "${GREEN}All results written to: $FULL_OUTPUT_DIR${NORMAL}" >&2
    elif [[ -n "$OUTPUT" ]]; then
        output_result > "$OUTPUT"
        echo -e "${GREEN}Results written to: $OUTPUT${NORMAL}" >&2
    else
        output_result
    fi
