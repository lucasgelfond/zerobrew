+++
title = "Configuration"
description = "Customize zerobrew's behavior with environment variables"
weight = 2
+++

## Environment Variables

zerobrew respects several environment variables for customization. Set these in your shell config (`~/.zshrc`, `~/.bashrc`, etc.) before the zerobrew PATH exports.

## Directory Configuration

{% param_field(path="ZEROBREW_ROOT", type="string", default="/opt/zerobrew") %}
The root data directory containing the store, database, cache, and prefix.

On Linux, defaults to `$XDG_DATA_HOME/zerobrew` or `~/.local/share/zerobrew`.
{% end %}

{% param_field(path="ZEROBREW_PREFIX", type="string", default="$ZEROBREW_ROOT/prefix") %}
The installation prefix where executables and libraries are linked.

This directory is added to your PATH.
{% end %}

{% param_field(path="ZEROBREW_DIR", type="string", default="~/.zerobrew") %}
The source code directory where zerobrew is cloned during installation.
{% end %}

{% param_field(path="ZEROBREW_BIN", type="string", default="~/.local/bin") %}
The directory where the `zb` binary is installed.
{% end %}

## Example Configurations

### Default (macOS)

```bash
# ~/.zshrc
export ZEROBREW_ROOT="/opt/zerobrew"
export ZEROBREW_PREFIX="/opt/zerobrew/prefix"
export ZEROBREW_DIR="$HOME/.zerobrew"
export ZEROBREW_BIN="$HOME/.local/bin"
export PATH="$ZEROBREW_BIN:$ZEROBREW_PREFIX/bin:$PATH"
```

### User-Local Installation

If you don't want to use `/opt`:

```bash
# ~/.zshrc
export ZEROBREW_ROOT="$HOME/.local/share/zerobrew"
export ZEROBREW_PREFIX="$HOME/.local"
export ZEROBREW_DIR="$HOME/.zerobrew"
export ZEROBREW_BIN="$HOME/.local/bin"
export PATH="$ZEROBREW_BIN:$ZEROBREW_PREFIX/bin:$PATH"
```

### XDG-Compliant (Linux)

```bash
# ~/.bashrc
export ZEROBREW_ROOT="${XDG_DATA_HOME:-$HOME/.local/share}/zerobrew"
export ZEROBREW_PREFIX="$HOME/.local"
export ZEROBREW_DIR="${XDG_DATA_HOME:-$HOME/.local/share}/zerobrew/src"
export ZEROBREW_BIN="$HOME/.local/bin"
export PATH="$ZEROBREW_BIN:$ZEROBREW_PREFIX/bin:$PATH"
```

## CLI Flags

You can also override directories per-command:

```bash
# Use a custom root directory
zb --root /custom/path install jq

# Use a custom prefix
zb --prefix /custom/prefix install jq

# Set concurrency (default: 48)
zb --concurrency 24 install ffmpeg
```

## Concurrency

zerobrew uses parallel downloads by default with 48 concurrent connections. Adjust this based on your network:

```bash
# Lower concurrency for slower connections
zb --concurrency 8 install ffmpeg

# Higher concurrency for fast connections
zb --concurrency 64 install ffmpeg
```

## pkg-config

zerobrew automatically sets up `PKG_CONFIG_PATH` during installation:

```bash
export PKG_CONFIG_PATH="$ZEROBREW_PREFIX/lib/pkgconfig:${PKG_CONFIG_PATH:-}"
```

This allows compilers to find libraries installed by zerobrew.

## Storage Layout Reference

| Directory | Purpose |
|-----------|---------|
| `$ZEROBREW_ROOT/store/` | SHA-256 addressable package storage |
| `$ZEROBREW_ROOT/db/` | SQLite database |
| `$ZEROBREW_ROOT/cache/` | Downloaded bottle blobs |
| `$ZEROBREW_ROOT/locks/` | Per-entry file locks |
| `$ZEROBREW_PREFIX/bin/` | Symlinked executables |
| `$ZEROBREW_PREFIX/Cellar/` | Materialized packages |
| `$ZEROBREW_PREFIX/lib/` | Shared libraries |
| `$ZEROBREW_PREFIX/include/` | Header files |
| `$ZEROBREW_PREFIX/share/` | Shared data |
| `$ZEROBREW_PREFIX/opt/` | Symlinked package directories |
