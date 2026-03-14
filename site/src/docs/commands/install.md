---
layout: "layouts/docs-page.njk"
title: "install"
description: "Install one or more packages"
weight: 2
---

{% from "components/docs/shortcodes/macros.njk" import note, info, warning, tip, card_group, card, tabs, tab, accordion_group, accordion, steps, step, param_fields, param_field %}

## Usage

```bash
zb install [OPTIONS] <formula>...
```

## Description

Downloads and installs a package and all its dependencies. zerobrew resolves the dependency tree, downloads bottles in parallel, and links executables to your PATH.

## Arguments

{% call param_fields() %}
{% call param_field("formula", "string", "", true) %}
One or more formula names. Uses Homebrew core formula naming.
{% endcall %}
{% endcall %}

## Options

{% call param_fields() %}
{% call param_field("--no-link", "boolean", "", false) %}
Install to the store but don't create symlinks. Useful for dependencies you don't need in PATH.
{% endcall %}
{% endcall %}

## Examples

### Install a single package

```bash
zb install jq
```

Output:
```
==> Installing jq...
==> Resolving dependencies (2 packages)...
    oniguruma 6.9.8
    jq 1.7.1
==> Downloading and installing...
    oniguruma       ━━━━━━━━━━━━━━━━━━━━━━━━━   1.2MB/1.2MB    0s
    jq              ━━━━━━━━━━━━━━━━━━━━━━━━━   402KB/402KB    0s
    oniguruma       ✓ installed
    jq              ✓ installed

==> Installed 2 packages in 0.45s
```

### Install multiple packages

```bash
zb install wget git ffmpeg sqlite
```

### Install without linking

```bash
zb install openssl --no-link
```

## How It Works

{% call steps() %}
{% call step("Resolve") %}
Fetches formula metadata from Homebrew's API and builds a dependency graph.
{% endcall %}
{% call step("Check Store") %}
Checks if each package already exists in the content-addressable store.
{% endcall %}
{% call step("Download") %}
Downloads missing bottles in parallel from Homebrew's CDN.
{% endcall %}
{% call step("Extract") %}
Streams extraction of tar.gz archives.
{% endcall %}
{% call step("Store") %}
Moves extracted files into `$ZEROBREW_ROOT/store/{sha256}/`.
{% endcall %}
{% call step("Materialize") %}
Materializes from store into Cellar (clonefile on macOS when available, otherwise hardlink/copy fallback).
{% endcall %}
{% call step("Link") %}
Creates symlinks in `bin/`, `lib/`, `include/`, etc.
{% endcall %}
{% endcall %}

## Warm vs Cold Installs

**Cold install**: Package not in store, must download

**Warm install**: Package already in store, instant materialization

```bash
# Cold install (downloads)
zb install sqlite
==> Installed 1 packages in 0.62s

# Uninstall
zb uninstall sqlite

# Warm install (from store)
zb install sqlite
==> Installed 1 packages in 0.15s  # 4x faster!
```

## Error Handling

If a package isn't found, zerobrew suggests using Homebrew:

```bash
zb install nonexistent-package
```

```
Error: Formula not found: nonexistent-package

This formula may not be available in zerobrew yet.
Try: brew install nonexistent-package
```

{% call tip() %}
zerobrew only supports core Homebrew formulas with bottles. If you need taps, casks, or source-only packages, use Homebrew.
{% endcall %}
