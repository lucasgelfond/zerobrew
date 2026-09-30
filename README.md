<div align="center">

<h2>zerobrew</h2>

<p align="center">
  <strong>English</strong> ·
  <a href="README.zh.md">中文</a>
</p>

[![Lint](https://github.com/lucasgelfond/zerobrew/actions/workflows/ci.yml/badge.svg)](https://github.com/lucasgelfond/zerobrew/actions/workflows/ci.yml)
[![Test](https://github.com/lucasgelfond/zerobrew/actions/workflows/test.yml/badge.svg)](https://github.com/lucasgelfond/zerobrew/actions/workflows/test.yml)
[![Release](https://img.shields.io/github/v/release/lucasgelfond/zerobrew?display_name=tag)](https://github.com/lucasgelfond/zerobrew/releases)
[![Discord](https://img.shields.io/badge/Discord-Join-5865F2?logo=discord&logoColor=white)](https://discord.gg/ZaPYwm9zaw)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](./LICENSE-MIT.md)
[![License: Apache 2.0](https://img.shields.io/badge/License-Apache_2.0-blue.svg)](./LICENSE-APACHE.md)

<img alt="zerobrew demo" src="./assets/zb-demo.gif" />

<p><strong>zerobrew brings uv-style architecture to Homebrew packages on macOS and Linux.</strong></p>

</div>

## Install

```bash
curl -fsSL https://zerobrew.rs/install | bash
```

The installer updates your shell config. After it finishes, restart your terminal
or run the `source` command it prints.

Or via Homebrew:

```bash
brew install lucasgelfond/zerobrew/zerobrew
```

## Update zerobrew

If you used the standalone installer, rerun it:

```bash
curl -fsSL https://zerobrew.rs/install | bash
zb --version
```

If you installed with Homebrew:

```bash
brew update && brew upgrade zerobrew
```

## Quick start

```bash
zb install jq                   # install one package
zb install wget git             # install multiple
zb bundle                       # install from Brewfile
zb bundle install -f myfile     # install from custom file
zb bundle dump                  # export installed packages to Brewfile
zb bundle dump -f out --force   # dump to custom file (overwrite)
zb uninstall jq                 # uninstall one package
zb outdated                     # list packages with newer versions
zb upgrade                      # upgrade all outdated packages
zb upgrade jq wget              # upgrade specific packages
zb reset                        # uninstall everything
zb gc                           # garbage collect unused store entries
zbx jq --version                # run without linking
```

## Performance snapshot

<div align="center">

| Package | Homebrew | ZB (cold) | ZB (warm) | Cold Speedup | Warm Speedup |
|---------|----------|-----------|-----------|--------------|--------------|
| **Overall (top 100)** | 452s | 226s | 59s | **2.0x** | **7.6x** |
| ffmpeg | 3034ms | 3481ms | 688ms | 0.9x | 4.4x |
| libsodium | 2353ms | 392ms | 130ms | 6.0x | 18.1x |
| sqlite | 2876ms | 625ms | 159ms | 4.6x | 18.1x |
| tesseract | 18950ms | 5536ms | 643ms | 3.4x | 29.5x |

</div>

These numbers are from January 2026, before v0.1.1, and are being re-measured ([#394](https://github.com/lucasgelfond/zerobrew/issues/394)). The benchmark used then didn't record versions, hardware or network, and didn't give both tools the same starting state: Homebrew kept its download cache and any dependencies left over from earlier packages, while zerobrew's cold runs started from scratch.

### Methodology

`just bench` installs each package four times. Every run starts with the package and all of its dependencies uninstalled.

- **Cold**: empty download cache.
- **Warm**: downloads from the cold run are still cached. zerobrew also keeps its extracted store, which is where most of its warm speedup comes from.

Homebrew runs with `HOMEBREW_NO_AUTO_UPDATE` and `HOMEBREW_NO_INSTALL_CLEANUP`, and its formula index is loaded before timing starts. Overall speedups compare total time across all packages. Each run records the zerobrew and Homebrew versions, OS, hardware and measured download bandwidth.

To reproduce, run `just bench --full results/` on a machine with nothing installed in Homebrew. It resets zerobrew and writes a README-ready table to `results/benchmark.md`.

## Relationship with Homebrew

zerobrew is more of a performance-optimized client for the Homebrew ecosystem. We rely on:
- Homebrew's formula definitions (homebrew-core)
- Homebrew's pre-built bottles when available
- Homebrew's package metadata and infrastructure

Our innovations focus on:
- Content-addressable storage for deduplication
- APFS clonefiles for zero-overhead copying
- Source build fallback using Homebrew's Ruby DSL

zerobrew is experimental. We recommend running it alongside Homebrew rather than as a replacement, and do _not_ 
recommend purging homebrew and replacing it with zerobrew unless you are absolutely sure about the implications of 
doing so. 

## Project status

<div align="center">
  <a href="https://star-history.dera.page/#lucasgelfond/zerobrew&Date">
    <picture>
      <source media="(prefers-color-scheme: dark)" srcset="https://star-history.dera.page/svg?repos=lucasgelfond/zerobrew&type=Date&theme=dark" />
      <img alt="Star History Chart" src="https://star-history.dera.page/svg?repos=lucasgelfond/zerobrew&type=Date" />
    </picture>
  </a>
</div>

- **Status:** Experimental, but already useful for many common Homebrew formulas.
- **Feedback:** If you hit incompatibilities, please open an issue or PR.
- **License:** Dual-licensed under [Apache 2.0](./LICENSE-APACHE.md) OR [MIT](./LICENSE-MIT.md), at your choice.
