+++
title = "Introduction"
description = "A faster, modern package manager"
weight = 1
+++

<img src="/images/demo.gif" alt="zerobrew demo" class="docs-hero-image" />

## What is zerobrew?

zerobrew applies [uv](https://github.com/astral-sh/uv)'s model to Mac packages. Packages live in a content-addressable store (by sha256), so reinstalls are instant. Downloads, extraction, and linking run in parallel with aggressive HTTP caching.

It pulls from Homebrew's CDN, so you can swap `brew` for `zb` with your existing commands.

{% card_group(cols=2) %}
{% card(title="Quickstart", icon="rocket", href="/docs/quickstart/") %}
Get up and running in under a minute.
{% end %}
{% card(title="Commands", icon="terminal", href="/docs/commands/overview/") %}
Explore all available commands.
{% end %}
{% card(title="Architecture", icon="sitemap", href="/docs/architecture/") %}
Learn how zerobrew works under the hood.
{% end %}
{% card(title="Migrate", icon="arrow", href="/docs/guides/migrating-from-homebrew/") %}
Move your packages from Homebrew.
{% end %}
{% end %}

## Performance

zerobrew delivers dramatic speedups — up to **5x cold** and **20x warm**.

| Package | Homebrew | ZB (cold) | ZB (warm) | Cold Speedup | Warm Speedup |
|---------|----------|-----------|-----------|--------------|--------------|
| **Overall** | 452s | 226s | 59s | **2.0x** | **7.6x** |
| ffmpeg | 3.03s | 3.48s | 0.69s | 0.9x | 4.4x |
| libsodium | 2.35s | 0.39s | 0.13s | 6.0x | 18.1x |
| sqlite | 2.88s | 0.63s | 0.16s | 4.6x | 18.1x |
| tesseract | 18.95s | 5.54s | 0.64s | 3.4x | 29.5x |

{% note() %}
"Cold" means the package isn't in the store. "Warm" means it's already cached.
{% end %}

## Why is it faster?

{% accordion_group() %}
{% accordion(title="Content-addressable store") %}
Packages are stored by sha256 hash at `/opt/zerobrew/store/{sha256}/`. Reinstalls are instant if the store entry exists.
{% end %}
{% accordion(title="APFS clonefile") %}
Materializing from store uses copy-on-write, meaning zero disk overhead.
{% end %}
{% accordion(title="Parallel downloads") %}
Deduplicates in-flight requests and races across CDN connections.
{% end %}
{% accordion(title="Streaming execution") %}
Downloads, extractions, and linking happen concurrently.
{% end %}
{% end %}

## Quick Example

```bash
# Install a package
zb install jq

# Install multiple packages
zb install wget git ffmpeg

# List installed packages
zb list

# Uninstall a package
zb uninstall jq
```

## Status

{% warning() %}
zerobrew is **experimental**. It works for most core Homebrew packages. Some formulas may need more work — please submit issues or PRs!
{% end %}

## License

zerobrew is dual-licensed under [Apache 2.0](https://github.com/lucasgelfond/zerobrew/blob/main/LICENSE-APACHE.md) or [MIT](https://github.com/lucasgelfond/zerobrew/blob/main/LICENSE-MIT.md), at your choice.
