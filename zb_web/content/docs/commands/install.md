+++
title = "install"
description = "Install one or more packages"
weight = 2
+++

## Usage

```bash
zb install <formula> [OPTIONS]
```

## Description

Downloads and installs a package and all its dependencies. zerobrew resolves the dependency tree, downloads bottles in parallel, and links executables to your PATH.

## Arguments

{% param_field(path="formula", type="string", required=true) %}
The name of the package to install. Uses the same names as Homebrew.
{% end %}

## Options

{% param_field(path="--no-link", type="boolean") %}
Install to the store but don't create symlinks. Useful for dependencies you don't need in PATH.
{% end %}

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

{% steps() %}
{% step(title="Resolve") %}
Fetches formula metadata from Homebrew's API and builds a dependency graph.
{% end %}
{% step(title="Check Store") %}
Checks if each package already exists in the content-addressable store.
{% end %}
{% step(title="Download") %}
Downloads missing bottles in parallel from Homebrew's CDN.
{% end %}
{% step(title="Extract") %}
Streams extraction of tar.gz archives.
{% end %}
{% step(title="Store") %}
Moves extracted files to `/opt/zerobrew/store/{sha256}/`.
{% end %}
{% step(title="Materialize") %}
Uses APFS clonefile to copy from store to Cellar (instant, zero disk overhead).
{% end %}
{% step(title="Link") %}
Creates symlinks in `bin/`, `lib/`, `include/`, etc.
{% end %}
{% end %}

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

{% tip() %}
zerobrew only supports core Homebrew formulas with bottles. If you need taps, casks, or source-only packages, use Homebrew.
{% end %}
