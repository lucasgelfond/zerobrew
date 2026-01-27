#!/bin/bash
# Benchmark: zerobrew vs homebrew
# Measures install time for both package managers

set -e

# Top 100 Homebrew packages from analytics (30d install count)
TOP_100_PACKAGES=(
    "ca-certificates"
    "openssl@3"
    "xz"
    "sqlite"
    "readline"
    "icu4c@78"
    "python@3.14"
    "awscli"
    "node"
    "harfbuzz"
    "ncurses"
    "gh"
    "pcre2"
    "libpng"
    "zstd"
    "glib"
    "lz4"
    "gettext"
    "libngtcp2"
    "libnghttp3"
    "pkgconf"
    "libunistring"
    "mpdecimal"
    "brotli"
    "jpeg-turbo"
    "xorgproto"
    "ffmpeg"
    "cmake"
    "libnghttp2"
    "go"
    "uv"
    "gmp"
    "libtiff"
    "fontconfig"
    "python@3.13"
    "git"
    "little-cms2"
    "dav1d"
    "openexr"
    "c-ares"
    "tesseract"
    "p11-kit"
    "imagemagick"
    "zlib"
    "libx11"
    "freetype"
    "protobuf"
    "gnupg"
    "openjph"
    "libtasn1"
    "ruby"
    "gnutls"
    "expat"
    "libsodium"
    "simdjson"
    "gemini-cli"
    "libarchive"
    "pyenv"
    "pixman"
    "curl"
    "opus"
    "unbound"
    "cairo"
    "pango"
    "leptonica"
    "libxcb"
    "jpeg-xl"
    "coreutils"
    "certifi"
    "krb5"
    "docker"
    "libheif"
    "webp"
    "libxext"
    "libxau"
    "gcc"
    "bzip2"
    "libxdmcp"
    "abseil"
    "xcbeautify"
    "libuv"
    "giflib"
    "utf8proc"
    "libxrender"
    "m4"
    "graphite2"
    "openjdk"
    "uvwasi"
    "libffi"
    "libdeflate"
    "llvm"
    "aom"
    "lzo"
    "libevent"
    "libgpg-error"
    "libidn2"
    "berkeley-db@5"
    "deno"
    "libedit"
    "oniguruma"
)

# Quick list for fast testing
QUICK_PACKAGES=(
    "jq"
    "tree"
    "htop"
    "bat"
    "fd"
    "ripgrep"
    "fzf"
    "wget"
    "curl"
    "git"
    "tmux"
    "zoxide"
    "openssl@3"
    "sqlite"
    "readline"
    "pcre2"
    "zstd"
    "lz4"
    "node"
    "go"
    "ruby"
    "gh"
)

# Defaults
COUNT=100
FORMAT="text"
OUTPUT_FILE=""
QUICK=false

# Parse args
while [[ $# -gt 0 ]]; do
    case $1 in
        -c|--count)
            COUNT="$2"
            shift 2
            ;;
        --format)
            FORMAT="$2"
            shift 2
            ;;
        -o|--output)
            OUTPUT_FILE="$2"
            shift 2
            ;;
        --quick)
            QUICK=true
            shift
            ;;
        -h|--help)
            echo "Usage: $0 [OPTIONS]"
            echo ""
            echo "Options:"
            echo "  -c, --count N     Number of packages to test (default: 100)"
            echo "  --format FORMAT   Output format: text, json, or html (default: text)"
            echo "  -o, --output FILE Write results to file instead of stdout"
            echo "  --quick           Use quick package list (fewer deps)"
            echo "  -h, --help        Show this help"
            exit 0
            ;;
        *)
            echo "Unknown option: $1"
            exit 1
            ;;
    esac
done

# Select package list
if [ "$QUICK" = true ]; then
    PACKAGES=("${QUICK_PACKAGES[@]:0:$COUNT}")
    SOURCE="quick"
else
    PACKAGES=("${TOP_100_PACKAGES[@]:0:$COUNT}")
    SOURCE="top 100"
fi

# Colors (only for text output to terminal)
if [ "$FORMAT" = "text" ] && [ -z "$OUTPUT_FILE" ]; then
    RED='\033[0;31m'
    GREEN='\033[0;32m'
    YELLOW='\033[1;33m'
    BLUE='\033[0;34m'
    NC='\033[0m'
else
    RED=''
    GREEN=''
    YELLOW=''
    BLUE=''
    NC=''
fi

# Get time in milliseconds
get_ms() {
    python3 -c 'import time; print(int(time.time() * 1000))'
}

# Results storage
declare -a RESULT_NAMES
declare -a RESULT_BREW
declare -a RESULT_ZB_COLD
declare -a RESULT_ZB_WARM
declare -a RESULT_SPEEDUP
declare -a FAILED_NAMES
declare -a FAILED_REASONS

PASSED=0
FAILED=0

echo -e "${BLUE}Running benchmark suite (${#PACKAGES[@]} packages from $SOURCE list)...${NC}" >&2
echo "" >&2

for i in "${!PACKAGES[@]}"; do
    pkg="${PACKAGES[$i]}"
    idx=$((i + 1))

    echo -e "${YELLOW}[$idx/${#PACKAGES[@]}] Benchmarking: $pkg${NC}" >&2

    # Uninstall from homebrew first (ignore errors)
    brew uninstall --ignore-dependencies "$pkg" 2>/dev/null || true
    zb uninstall "$pkg" 2>/dev/null || true

    # Clean zerobrew caches for cold test
    rm -rf /opt/zerobrew/db /opt/zerobrew/cache /opt/zerobrew/store 2>/dev/null || true

    # Time homebrew install
    echo "  Installing with homebrew..." >&2
    BREW_START=$(get_ms)
    if brew install "$pkg" >/dev/null 2>&1; then
        BREW_END=$(get_ms)
        BREW_MS=$((BREW_END - BREW_START))
        echo -e "  ${GREEN}Homebrew: ${BREW_MS}ms${NC}" >&2
    else
        BREW_MS=""
        echo -e "  ${RED}Homebrew: FAILED${NC}" >&2
        FAILED_NAMES+=("$pkg")
        FAILED_REASONS+=("brew install failed")
        ((FAILED++))
        echo "" >&2
        continue
    fi

    # Uninstall from homebrew for fair zb test
    brew uninstall --ignore-dependencies "$pkg" 2>/dev/null || true

    # Time zerobrew cold install
    echo "  Installing with zerobrew (cold)..." >&2
    ZB_COLD_START=$(get_ms)
    if zb install "$pkg" >/dev/null 2>&1; then
        ZB_COLD_END=$(get_ms)
        ZB_COLD_MS=$((ZB_COLD_END - ZB_COLD_START))
        echo -e "  ${GREEN}Zerobrew cold: ${ZB_COLD_MS}ms${NC}" >&2
    else
        ZB_COLD_MS=""
        echo -e "  ${RED}Zerobrew cold: FAILED${NC}" >&2
        FAILED_NAMES+=("$pkg")
        FAILED_REASONS+=("zb install failed")
        ((FAILED++))
        echo "" >&2
        continue
    fi

    # Uninstall for warm test (keep cache)
    zb uninstall "$pkg" >/dev/null 2>&1 || true

    # Time zerobrew warm install
    echo "  Installing with zerobrew (warm)..." >&2
    ZB_WARM_START=$(get_ms)
    if zb install "$pkg" >/dev/null 2>&1; then
        ZB_WARM_END=$(get_ms)
        ZB_WARM_MS=$((ZB_WARM_END - ZB_WARM_START))
        echo -e "  ${GREEN}Zerobrew warm: ${ZB_WARM_MS}ms${NC}" >&2
    else
        ZB_WARM_MS="0"
        echo -e "  ${RED}Zerobrew warm: FAILED${NC}" >&2
    fi

    # Calculate speedup
    if [ -n "$BREW_MS" ] && [ -n "$ZB_COLD_MS" ] && [ "$ZB_COLD_MS" -gt 0 ]; then
        SPEEDUP=$(echo "scale=1; $BREW_MS / $ZB_COLD_MS" | bc)
    else
        SPEEDUP="0"
    fi

    # Store results
    RESULT_NAMES+=("$pkg")
    RESULT_BREW+=("$BREW_MS")
    RESULT_ZB_COLD+=("$ZB_COLD_MS")
    RESULT_ZB_WARM+=("$ZB_WARM_MS")
    RESULT_SPEEDUP+=("$SPEEDUP")
    ((PASSED++))

    echo "" >&2
done

# Clean up all installed packages
echo -e "${YELLOW}Cleaning up...${NC}" >&2
for pkg in "${PACKAGES[@]}"; do
    brew uninstall --ignore-dependencies "$pkg" 2>/dev/null || true
    zb uninstall "$pkg" 2>/dev/null || true
done
zb uninstall 2>/dev/null || true

# Calculate averages
TOTAL_BREW=0
TOTAL_ZB_COLD=0
TOTAL_ZB_WARM=0

for i in "${!RESULT_NAMES[@]}"; do
    TOTAL_BREW=$((TOTAL_BREW + RESULT_BREW[$i]))
    TOTAL_ZB_COLD=$((TOTAL_ZB_COLD + RESULT_ZB_COLD[$i]))
    TOTAL_ZB_WARM=$((TOTAL_ZB_WARM + RESULT_ZB_WARM[$i]))
done

if [ "$PASSED" -gt 0 ] && [ "$TOTAL_ZB_COLD" -gt 0 ]; then
    AVG_COLD_SPEEDUP=$(echo "scale=1; $TOTAL_BREW / $TOTAL_ZB_COLD" | bc)
else
    AVG_COLD_SPEEDUP="0"
fi

if [ "$PASSED" -gt 0 ] && [ "$TOTAL_ZB_WARM" -gt 0 ]; then
    AVG_WARM_SPEEDUP=$(echo "scale=1; $TOTAL_BREW / $TOTAL_ZB_WARM" | bc)
else
    AVG_WARM_SPEEDUP="0"
fi

# Generate output based on format
generate_text() {
    echo ""
    echo "=== Suite Summary ==="
    echo "Tested: ${#PACKAGES[@]} packages"
    echo "Passed: $PASSED"
    echo "Failed: $FAILED"
    echo ""
    echo "Performance:"
    echo "  Avg cold speedup vs Homebrew: ${AVG_COLD_SPEEDUP}x"
    echo "  Avg warm speedup vs Homebrew: ${AVG_WARM_SPEEDUP}x"
    echo ""
    echo "Results:"
    printf "%-20s %12s %12s %12s %10s\n" "Package" "Brew (ms)" "ZB Cold" "ZB Warm" "Speedup"
    printf "%s\n" "----------------------------------------------------------------------"

    for i in "${!RESULT_NAMES[@]}"; do
        printf "%-20s %12s %12s %12s %9sx\n" \
            "${RESULT_NAMES[$i]}" \
            "${RESULT_BREW[$i]}" \
            "${RESULT_ZB_COLD[$i]}" \
            "${RESULT_ZB_WARM[$i]}" \
            "${RESULT_SPEEDUP[$i]}"
    done

    if [ ${#FAILED_NAMES[@]} -gt 0 ]; then
        echo ""
        echo "Failed packages:"
        for i in "${!FAILED_NAMES[@]}"; do
            echo "  ${FAILED_NAMES[$i]} - ${FAILED_REASONS[$i]}"
        done
    fi
    echo ""
    echo "Done."
}

generate_json() {
    echo "{"
    echo '  "results": ['

    for i in "${!RESULT_NAMES[@]}"; do
        comma=""
        if [ $i -lt $((${#RESULT_NAMES[@]} - 1)) ]; then
            comma=","
        fi
        echo "    {"
        echo "      \"name\": \"${RESULT_NAMES[$i]}\","
        echo "      \"homebrew_cold_ms\": ${RESULT_BREW[$i]},"
        echo "      \"zerobrew_cold_ms\": ${RESULT_ZB_COLD[$i]},"
        echo "      \"zerobrew_warm_ms\": ${RESULT_ZB_WARM[$i]},"
        echo "      \"cold_speedup\": ${RESULT_SPEEDUP[$i]}"
        echo "    }$comma"
    done

    echo '  ],'
    echo '  "failures": ['

    for i in "${!FAILED_NAMES[@]}"; do
        comma=""
        if [ $i -lt $((${#FAILED_NAMES[@]} - 1)) ]; then
            comma=","
        fi
        echo "    [\"${FAILED_NAMES[$i]}\", \"${FAILED_REASONS[$i]}\"]$comma"
    done

    echo '  ],'
    echo '  "summary": {'
    echo "    \"tested\": ${#PACKAGES[@]},"
    echo "    \"passed\": $PASSED,"
    echo "    \"failed\": $FAILED,"
    echo "    \"avg_cold_speedup\": $AVG_COLD_SPEEDUP,"
    echo "    \"avg_warm_speedup\": $AVG_WARM_SPEEDUP"
    echo '  }'
    echo "}"
}

generate_html() {
    cat <<'HTMLHEAD'
<!DOCTYPE html>
<html>
<head>
    <title>Zerobrew Benchmark Results</title>
    <style>
        body { font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif; margin: 40px; background: #f5f5f5; }
        .container { max-width: 1000px; margin: 0 auto; background: white; padding: 30px; border-radius: 8px; box-shadow: 0 2px 4px rgba(0,0,0,0.1); }
        h1 { color: #333; border-bottom: 2px solid #0066cc; padding-bottom: 10px; }
        .summary { display: grid; grid-template-columns: repeat(auto-fit, minmax(150px, 1fr)); gap: 20px; margin: 20px 0; }
        .stat { background: #f8f9fa; padding: 20px; border-radius: 8px; text-align: center; }
        .stat-value { font-size: 2em; font-weight: bold; color: #0066cc; }
        .stat-label { color: #666; margin-top: 5px; }
        table { width: 100%; border-collapse: collapse; margin: 20px 0; }
        th, td { padding: 12px; text-align: left; border-bottom: 1px solid #ddd; }
        th { background: #0066cc; color: white; }
        tr:hover { background: #f5f5f5; }
        .speedup { font-weight: bold; color: #28a745; }
        .failed { color: #dc3545; }
        .timestamp { color: #999; font-size: 0.9em; margin-top: 20px; }
    </style>
</head>
<body>
    <div class="container">
        <h1>Zerobrew Benchmark Results</h1>
        <div class="summary">
            <div class="stat">
HTMLHEAD
    echo "                <div class=\"stat-value\">${#PACKAGES[@]}</div>"
    cat <<'HTML1'
                <div class="stat-label">Packages Tested</div>
            </div>
            <div class="stat">
HTML1
    echo "                <div class=\"stat-value\">$PASSED</div>"
    cat <<'HTML2'
                <div class="stat-label">Passed</div>
            </div>
            <div class="stat">
HTML2
    echo "                <div class=\"stat-value\">${AVG_COLD_SPEEDUP}x</div>"
    cat <<'HTML3'
                <div class="stat-label">Avg Cold Speedup</div>
            </div>
            <div class="stat">
HTML3
    echo "                <div class=\"stat-value\">${AVG_WARM_SPEEDUP}x</div>"
    cat <<'HTML4'
                <div class="stat-label">Avg Warm Speedup</div>
            </div>
        </div>

        <h2>Results</h2>
        <table>
            <thead>
                <tr>
                    <th>Package</th>
                    <th>Homebrew (ms)</th>
                    <th>ZB Cold (ms)</th>
                    <th>ZB Warm (ms)</th>
                    <th>Speedup</th>
                </tr>
            </thead>
            <tbody>
HTML4

    for i in "${!RESULT_NAMES[@]}"; do
        echo "                <tr>"
        echo "                    <td>${RESULT_NAMES[$i]}</td>"
        echo "                    <td>${RESULT_BREW[$i]}</td>"
        echo "                    <td>${RESULT_ZB_COLD[$i]}</td>"
        echo "                    <td>${RESULT_ZB_WARM[$i]}</td>"
        echo "                    <td class=\"speedup\">${RESULT_SPEEDUP[$i]}x</td>"
        echo "                </tr>"
    done

    echo "            </tbody>"
    echo "        </table>"

    if [ ${#FAILED_NAMES[@]} -gt 0 ]; then
        echo "        <h2>Failed Packages</h2>"
        echo "        <ul>"
        for i in "${!FAILED_NAMES[@]}"; do
            echo "            <li class=\"failed\"><strong>${FAILED_NAMES[$i]}</strong>: ${FAILED_REASONS[$i]}</li>"
        done
        echo "        </ul>"
    fi

    TIMESTAMP=$(date +%s)
    echo "        <div class=\"timestamp\">Generated: $TIMESTAMP</div>"
    echo "    </div>"
    echo "</body>"
    echo "</html>"
}

# Output results
case "$FORMAT" in
    json)
        if [ -n "$OUTPUT_FILE" ]; then
            generate_json > "$OUTPUT_FILE"
            echo -e "${GREEN}Results written to: $OUTPUT_FILE${NC}" >&2
        else
            generate_json
        fi
        ;;
    html)
        if [ -n "$OUTPUT_FILE" ]; then
            generate_html > "$OUTPUT_FILE"
            echo -e "${GREEN}Results written to: $OUTPUT_FILE${NC}" >&2
        else
            generate_html
        fi
        ;;
    *)
        if [ -n "$OUTPUT_FILE" ]; then
            generate_text > "$OUTPUT_FILE"
            echo -e "${GREEN}Results written to: $OUTPUT_FILE${NC}" >&2
        else
            generate_text
        fi
        ;;
esac

echo -e "${GREEN}Done.${NC}" >&2
