+++
title = "Commands Overview"
description = "Complete reference for all zerobrew commands"
weight = 1
+++

## Command Structure

```bash
zb [OPTIONS] <COMMAND>
```

### Global Options

| Option | Description |
|--------|-------------|
| `--root <PATH>` | Override the data directory (env: `ZEROBREW_ROOT`) |
| `--prefix <PATH>` | Override the installation prefix (env: `ZEROBREW_PREFIX`) |
| `--concurrency <N>` | Set download concurrency (default: 48) |
| `--version` | Print version information |
| `--help` | Print help |

## Commands

{% card_group(cols=2) %}
{% card(title="install", icon="download", href="/docs/commands/install/") %}
Install one or more packages.
{% end %}
{% card(title="uninstall", icon="trash", href="/docs/commands/uninstall/") %}
Remove a package.
{% end %}
{% card(title="list", icon="list", href="/docs/commands/list/") %}
Show installed packages.
{% end %}
{% card(title="info", icon="info", href="/docs/commands/info/") %}
Display package information.
{% end %}
{% card(title="migrate", icon="swap", href="/docs/commands/migrate/") %}
Migrate packages from Homebrew.
{% end %}
{% card(title="gc", icon="broom", href="/docs/commands/gc/") %}
Garbage collect unused store entries.
{% end %}
{% card(title="reset", icon="reset", href="/docs/commands/reset/") %}
Uninstall all packages.
{% end %}
{% card(title="init", icon="setup", href="/docs/commands/init/") %}
Initialize zerobrew directories.
{% end %}
{% card(title="completion", icon="terminal", href="/docs/commands/completion/") %}
Generate shell completions.
{% end %}
{% end %}

## Quick Reference

```bash
# Install packages
zb install jq
zb install wget git ffmpeg

# Manage packages
zb uninstall jq
zb list
zb info sqlite

# Migration & maintenance
zb migrate
zb gc
zb reset

# Setup
zb init
zb completion zsh
```

## Command Comparison with Homebrew

| Task | Homebrew | zerobrew |
|------|----------|----------|
| Install | `brew install jq` | `zb install jq` |
| Uninstall | `brew uninstall jq` | `zb uninstall jq` |
| List | `brew list` | `zb list` |
| Info | `brew info jq` | `zb info jq` |
| Update | `brew upgrade` | Re-run `zb install` |
| Cleanup | `brew cleanup` | `zb gc` |
