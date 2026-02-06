<div align="center">

# zerobrew

[![Lint](https://github.com/lucasgelfond/zerobrew/actions/workflows/ci.yml/badge.svg)](https://github.com/lucasgelfond/zerobrew/actions/workflows/ci.yml)
[![Test](https://github.com/lucasgelfond/zerobrew/actions/workflows/test.yml/badge.svg)](https://github.com/lucasgelfond/zerobrew/actions/workflows/test.yml)

</div>

## Install

```bash
curl -sSL https://raw.githubusercontent.com/lucasgelfond/zerobrew/main/install.sh | bash
```

After install, run the export command it prints, or restart your terminal.

Join the [Discord](https://discord.gg/UxAAvZ93) for support / discussion.

## About

A fast, modern package manager.

![zb demo](zb-demo.gif)

zerobrew applies [uv](https://github.com/astral-sh/uv)'s model to Mac packages. Packages live in a content-addressable store (by sha256), so reinstalls are instant. Downloads, extraction, and linking run in parallel with aggressive HTTP caching. It pulls from Homebrew's CDN, so you can    swap `brew` for `zb` with your existing commands. 

This leads to dramatic speedups, up to 5x cold and 20x warm. Full benchmarks [here](benchmark-results.txt).

| Package | Homebrew | ZB (cold) | ZB (warm) | Cold Speedup | Warm Speedup |
|---------|----------|-----------|-----------|--------------|--------------|
| **Overall (top 100)** | 452s | 226s | 59s | **2.0x** | **7.6x** |
| ffmpeg | 3034ms | 3481ms | 688ms | 0.9x | 4.4x |
| libsodium | 2353ms | 392ms | 130ms | 6.0x | 18.1x |
| sqlite | 2876ms | 625ms | 159ms | 4.6x | 18.1x |
| tesseract | 18950ms | 5536ms | 643ms | 3.4x | 29.5x | 

##  Using `zb`

```bash
zb install jq                   # install jq
zb install wget git             # install multiple
zb install --file Brewfile      # install from a manifest
zb bundle                       # shorthand for Brewfile in current dir
zb uninstall jq                 # uninstall
zb reset                        # uninstall everything
zb gc                           # garbage collect unused store entries
zbx jq --version                # run without linking
```

### Brewfile manifests

Create a plain text manifest (compatible with Homebrew's Brewfile) listing one formula per line:

```text
# Brewfile
jq
wget
git
```

Blank lines and comments (lines starting with `#`) are ignored. Install everything in the manifest with `zb install --file Brewfile` or use `zb bundle` to read the default `./Brewfile`.

## Why is it faster?

- **Content-addressable store**: packages are stored by sha256 hash (at `/opt/zerobrew/store/{sha256}/`). Reinstalls are instant if the store entry exists.
- **APFS clonefile**: materializing from store uses copy-on-write (zero disk overhead).
- **Parallel downloads**: deduplicates in-flight requests, races across CDN connections.
- **Streaming execution**: downloads, extractions, and linking happen concurrently.

## Notes on LLMs

I spent a lot of time thinking through this architecture, testing, and debugging. I also used Claude Opus 4.5 to write much of the code here. I am a big believer in language models for coding, especially when they are given a precise spec and work with human input! See some of the discussion about this [on Reddit](https://www.reddit.com/r/rust/comments/1qn2aev/zerobrew_is_a_rustbased_520x_faster_dropin/) that convinced me it was worth adding to the README. A lot of people I respect, [including the developers of uv](https://x.com/charliermarsh/status/2007117912801427905) are doing similar sorts of development, I don't think this is a particularly crazy practice in 2026. 


## Storage layout

```sh
/opt/zerobrew/      # Data directory (default: $ZEROBREW_ROOT)
├── store/          # sha256-addressable packages
├── db/             # sqlite database
├── cache/          # downloaded bottle blobs
├── locks/          # per-entry file locks
└── prefix/         # $ZEROBREW_PREFIX (default: $ZEROBREW_ROOT/prefix)
    ├── bin/        # symlinked executables
    ├── Cellar/     # materialized packages
    ├── lib/
    ├── include/
    ├── share/
    └── opt/        # symlinked package directories
~/.zerobrew/        # $ZEROBREW_DIR (source code, default: ~/.zerobrew)
~/.local/bin/zb     # $ZEROBREW_BIN (binary, default: ~/.local/bin)
```

All variables are respected by both the install script and `zb` CLI:

- `ZEROBREW_ROOT`
- `ZEROBREW_PREFIX`
- `ZEROBREW_DIR`
- `ZEROBREW_BIN`

## Build from source 

```bash
cargo build --release
cargo install --path zb_cli
```

## Benchmarking

```bash
just bench --quick          # quick test (22 packages)
just bench --full results/  # 100-package benchmark
just bench --help           # show help
```

## Status

Experimental. works for most core homebrew packages. Some formulas may need more work - please submit issues / PRs! 


## License

zerobrew is dual-licensed, usable under both [Apache](./LICENSE-APACHE.md) OR [MIT](./LICENSE-MIT.md), at your choice.
