---
layout: "layouts/docs-page.njk"
title: "Commands"
description: "Complete reference for all zerobrew commands"
weight: 3
---

{% from "components/docs/shortcodes/macros.njk" import note, info, warning, tip, card_group, card, tabs, tab, accordion_group, accordion, steps, step, param_fields, param_field %}

## Command Structure

```bash
zb [OPTIONS] <COMMAND>
```

### Global Options

| Option | Description |
|--------|-------------|
| `--root <PATH>` | Override data root (env: `ZEROBREW_ROOT`) |
| `--prefix <PATH>` | Override install prefix (env: `ZEROBREW_PREFIX`) |
| `--concurrency <N>` | Download concurrency (default: `20`) |
| `--auto-init` | Auto-run `zb init` in non-interactive workflows (env: `ZEROBREW_AUTO_INIT`) |
| `--version` | Print version |
| `--help` | Print help |

## Quick Reference

```bash
# Install formulas
zb install jq
zb install wget git ffmpeg

# Install from manifest
zb bundle -f Brewfile

# Manage installs
zb uninstall jq
zb uninstall --all
zb list
zb info sqlite

# Setup and maintenance
zb init
zb completion zsh
zb gc
zb reset

# Run without linking
zbx jq --version
```

## Homebrew Mapping

| Task | Homebrew | zerobrew |
|------|----------|----------|
| Install | `brew install jq` | `zb install jq` |
| Install from Brewfile | `brew bundle` | `zb bundle -f Brewfile` |
| Uninstall | `brew uninstall jq` | `zb uninstall jq` |
| List | `brew list` | `zb list` |
| Info | `brew info jq` | `zb info jq` |
| Cleanup | `brew cleanup` | `zb gc` |

## Commands
