---
layout: "layouts/docs-page.njk"
title: "Configuration"
description: "Customize zerobrew with env vars and global CLI options"
weight: 2
---

{% from "components/docs/shortcodes/macros.njk" import note, info, warning, tip, card_group, card, tabs, tab, accordion_group, accordion, steps, step, param_fields, param_field %}

## Environment Variables

Set these in your shell config before running zerobrew commands.

{% call param_fields() %}
{% call param_field("ZEROBREW_ROOT", "path", "macOS: /opt/zerobrew; Linux: $XDG_DATA_HOME/zerobrew", false) %}Root data directory.{% endcall %}
{% call param_field("ZEROBREW_PREFIX", "path", "$ZEROBREW_ROOT/prefix", false) %}Install prefix used for linked files and runtime.{% endcall %}
{% call param_field("ZEROBREW_DIR", "path", "$HOME/.zerobrew", false) %}Local source checkout location used by installer workflows.{% endcall %}
{% call param_field("ZEROBREW_BIN", "path", "$HOME/.local/bin", false) %}Directory where `zb` and `zbx` binaries are installed.{% endcall %}
{% call param_field("ZEROBREW_AUTO_INIT", "boolean", "false", false) %}Enable non-interactive auto-initialization (`--auto-init`).{% endcall %}
{% endcall %}

## Global CLI Options

```bash
zb --root /custom/root --prefix /custom/prefix --concurrency 12 <command>
```

| Option | Default | Notes |
|--------|---------|-------|
| `--root` | from `ZEROBREW_ROOT` | data root override |
| `--prefix` | from `ZEROBREW_PREFIX` (or computed) | install prefix override |
| `--concurrency` | `20` | must be >= 1 |
| `--auto-init` | `false` | useful for CI/non-interactive invocations |

## Prefix behavior

- On **macOS**, default prefix is the same as root to satisfy Mach-O path constraints.
- On **Linux**, default prefix is `$ZEROBREW_ROOT/prefix`.

{% call warning() %}
On macOS, very long custom prefixes can break path-sensitive formulas. Prefer short prefixes.
{% endcall %}

## pkg-config

`zb init` exports:

```bash
export PKG_CONFIG_PATH="$ZEROBREW_PREFIX/lib/pkgconfig:${PKG_CONFIG_PATH:-}"
```

## Layout reference

| Directory | Purpose |
|-----------|---------|
| `$ZEROBREW_ROOT/store/` | content-addressed package storage |
| `$ZEROBREW_ROOT/db/` | SQLite metadata database |
| `$ZEROBREW_ROOT/cache/` | downloaded bottle blobs |
| `$ZEROBREW_ROOT/locks/` | file locks |
| `$ZEROBREW_PREFIX/bin/` | linked executables |
| `$ZEROBREW_PREFIX/Cellar/` | materialized formula trees |
