<div align="center">

<img alt="zerobrew banner" src="./assets/banner.svg" width="100%" />

<p><strong>Zerobrew brings uv-style architecture to Homebrew packages on macOS and Linux.</strong></p>

[![Lint](https://github.com/lucasgelfond/zerobrew/actions/workflows/ci.yml/badge.svg)](https://github.com/lucasgelfond/zerobrew/actions/workflows/ci.yml)
[![Test](https://github.com/lucasgelfond/zerobrew/actions/workflows/test.yml/badge.svg)](https://github.com/lucasgelfond/zerobrew/actions/workflows/test.yml)
[![Release](https://img.shields.io/github/v/release/lucasgelfond/zerobrew?display_name=tag)](https://github.com/lucasgelfond/zerobrew/releases)
[![Discord](https://img.shields.io/badge/Discord-Join-5865F2?logo=discord&logoColor=white)](https://discord.gg/ZaPYwm9zaw)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](./LICENSE-MIT.md)
[![License: Apache 2.0](https://img.shields.io/badge/License-Apache_2.0-blue.svg)](./LICENSE-APACHE.md)

</div>

## Install

```bash
curl -fsSL https://zerobrew.rs/install | bash
```

After install, run the `export` command it prints (or restart your terminal).

## Demo

<div align="center">
  <img alt="zerobrew demo" src="./assets/zb-demo.gif" />
</div>

## Quick start

```bash
zb install jq                   # install one package
zb install wget git             # install multiple
zb install --file Brewfile      # install from a manifest
zb bundle                       # shorthand for ./Brewfile
zb uninstall jq                 # uninstall one package
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

## Project status

<div align="center">
  <a href="https://star-history.com/#lucasgelfond/zerobrew&Date">
    <picture>
      <source media="(prefers-color-scheme: dark)" srcset="https://api.star-history.com/svg?repos=lucasgelfond/zerobrew&type=Date&theme=dark" />
      <img alt="Star History Chart" src="https://api.star-history.com/svg?repos=lucasgelfond/zerobrew&type=Date" />
    </picture>
  </a>
</div>

- **Status:** Experimental, but already useful for many common Homebrew formulas.
- **Feedback:** If you hit incompatibilities, please open an issue or PR.
- **License:** Dual-licensed under [Apache 2.0](./LICENSE-APACHE.md) OR [MIT](./LICENSE-MIT.md), at your choice.
