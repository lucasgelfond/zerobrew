+++
title = "Benchmarking"
description = "Measure zerobrew's performance against Homebrew"
weight = 2
+++

## Running Benchmarks

zerobrew includes a benchmarking script to compare performance with Homebrew:

```bash
./benchmark.sh
```

This runs a 100-package installation suite and reports timings.

## Benchmark Options

```bash
# Default: 100-package benchmark
./benchmark.sh

# HTML report
./benchmark.sh --format html -o results.html

# JSON output
./benchmark.sh --format json -o results.json

# Quick test (22 packages)
./benchmark.sh -c 20 --quick

# Show help
./benchmark.sh -h
```

## Understanding Results

The benchmark measures:

| Metric | Description |
|--------|-------------|
| **Cold** | Package not in store, requires download |
| **Warm** | Package already in store, instant materialization |
| **Speedup** | Ratio of Homebrew time to zerobrew time |

### Example Output

```
| Package         | Homebrew | ZB (cold) | ZB (warm) | Cold Speedup | Warm Speedup |
|-----------------|----------|-----------|-----------|--------------|--------------|
| Overall (100)   | 452s     | 226s      | 59s       | 2.0x         | 7.6x         |
| ffmpeg          | 3034ms   | 3481ms    | 688ms     | 0.9x         | 4.4x         |
| libsodium       | 2353ms   | 392ms     | 130ms     | 6.0x         | 18.1x        |
| sqlite          | 2876ms   | 625ms     | 159ms     | 4.6x         | 18.1x        |
| tesseract       | 18950ms  | 5536ms    | 643ms     | 3.4x         | 29.5x        |
```

## Typical Results

| Scenario | Expected Speedup |
|----------|-----------------|
| Cold install (first time) | 1.5x - 5x |
| Warm install (cached) | 5x - 25x |
| Large packages (ffmpeg) | ~1x cold, 4x warm |
| Small packages (jq) | 5x+ cold, 20x+ warm |

{% info() %}
Cold performance depends on network speed. Warm performance is consistently fast because it's a local operation.
{% end %}

## Why Some Packages Show 1x Cold

Large packages like `ffmpeg` may show similar cold times because:

1. Download time dominates (same CDN, same file)
2. These packages have many dependencies
3. Homebrew's parallel downloads are also efficient

The real win is **warm performance** — once packages are in the store, reinstalls are nearly instant.

## Running Consistent Benchmarks

For accurate results:

```bash
# Clear zerobrew store
zb reset -y

# Clear Homebrew
brew uninstall --force $(brew list --formula)
brew cleanup --prune=all

# Run benchmark
./benchmark.sh
```

{% warning() %}
This removes all packages. Only run on a test machine or if you're prepared to reinstall.
{% end %}

## Interpreting for Your Workflow

Consider your usage patterns:

| If you... | zerobrew benefit |
|-----------|-----------------|
| Reinstall frequently | High (warm installs) |
| Use CI/CD | High (cached store) |
| Install once | Moderate (cold only) |
| Use many large packages | Moderate |
| Use many small packages | Very high |
