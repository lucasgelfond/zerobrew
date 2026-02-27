---
layout: "layouts/docs-page.njk"
title: "Introduction"
description: "A faster, modern package manager"
weight: 1
---

{% from "components/docs/shortcodes/macros.njk" import note, info, warning, tip, card_group, card, tabs, tab, accordion_group, accordion, steps, step, param_fields, param_field %}

<img src="/assets/images/demo.gif" alt="zerobrew demo" class="docs-hero-image" />

## What is zerobrew?

zerobrew applies [uv](https://github.com/astral-sh/uv)'s model to Mac packages. Packages live in a content-addressable store (by sha256), so reinstalls are instant. Downloads, extraction, and linking run in parallel with aggressive HTTP caching.

It pulls from Homebrew's CDN, so you can swap `brew` for `zb` with your existing commands.

{% call card_group(2) %}
{% call card("Quickstart", "/docs/getting-started/quickstart/", "rocket") %}
Get up and running in under a minute.
{% endcall %}
{% call card("Commands", "/docs/commands/", "terminal") %}
Explore all available commands.
{% endcall %}
{% call card("Architecture", "/docs/general/architecture/", "sitemap") %}
Learn how zerobrew works under the hood.
{% endcall %}
{% call card("Migrate", "/docs/guides/migrating-from-homebrew/", "arrow") %}
Move your packages from Homebrew.
{% endcall %}
{% endcall %}

## Performance

zerobrew delivers dramatic speedups — up to **5x cold** and **20x warm**.

| Package | Homebrew | ZB (cold) | ZB (warm) | Cold Speedup | Warm Speedup |
|---------|----------|-----------|-----------|--------------|--------------|
| **Overall** | 452s | 226s | 59s | **2.0x** | **7.6x** |
| ffmpeg | 3.03s | 3.48s | 0.69s | 0.9x | 4.4x |
| libsodium | 2.35s | 0.39s | 0.13s | 6.0x | 18.1x |
| sqlite | 2.88s | 0.63s | 0.16s | 4.6x | 18.1x |
| tesseract | 18.95s | 5.54s | 0.64s | 3.4x | 29.5x |

{% call note() %}
"Cold" means the package isn't in the store. "Warm" means it's already cached.
{% endcall %}

## Why is it faster?

{% call accordion_group() %}
{% call accordion("Content-addressable store") %}
Packages are stored by sha256 hash at `/opt/zerobrew/store/{sha256}/`. Reinstalls are instant if the store entry exists.
{% endcall %}
{% call accordion("APFS clonefile") %}
Materializing from store uses copy-on-write, meaning zero disk overhead.
{% endcall %}
{% call accordion("Parallel downloads") %}
Deduplicates in-flight requests and races across CDN connections.
{% endcall %}
{% call accordion("Streaming execution") %}
Downloads, extractions, and linking happen concurrently.
{% endcall %}
{% endcall %}

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

{% call warning() %}
zerobrew is **experimental**. It works for most core Homebrew packages. Some formulas may need more work — please submit issues or PRs!
{% endcall %}

## License

zerobrew is dual-licensed under [Apache 2.0](https://github.com/lucasgelfond/zerobrew/blob/main/LICENSE-APACHE.md) or [MIT](https://github.com/lucasgelfond/zerobrew/blob/main/LICENSE-MIT.md), at your choice.
