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

Get started here on our official [guide](https://zerobrew.mintlify.app/introduction)

## About

A faster, modern package manager.


![zb demo](zb-demo.gif)

zerobrew applies [uv](https://github.com/astral-sh/uv)'s model to Mac packages. Packages live in a content-addressable store (by sha256), so reinstalls are instant. Downloads, extraction, and linking run in parallel with aggressive HTTP caching. It pulls from Homebrew's CDN, so you can    swap `brew` for `zb` with your existing commands. 

## Status

Experimental. works for most core homebrew packages. Some formulas may need more work - please submit issues / PRs! 


## License

zerobrew is dual-licensed, usable under both [Apache](./LICENSE-APACHE.md) OR [MIT](./LICENSE-MIT.md), at your choice.
