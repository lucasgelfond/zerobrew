---
layout: "layouts/docs-page.njk"
title: "init"
description: "Initialize zerobrew directories and shell config"
weight: 10
---

{% from "components/docs/shortcodes/macros.njk" import note, info, warning, tip, card_group, card, tabs, tab, accordion_group, accordion, steps, step, param_fields, param_field %}

## Usage

```bash
zb init [OPTIONS]
```

## Description

Initializes zerobrew data directories and shell configuration.

- creates root/store/db/cache/locks
- creates prefix/bin and prefix/Cellar
- writes/updates a managed zerobrew block in your shell config (unless disabled)

If directory creation requires elevated permissions, `init` will request `sudo` and then `chown` to your user.

## Options

{% call param_fields() %}
{% call param_field("--no-modify-path", "boolean", "", false) %}Skip writing shell config and PATH exports.{% endcall %}
{% endcall %}

## Notes

- On macOS, long custom prefixes can break Mach-O path patching for some formulas.
- Keep prefix length at or below `/opt/homebrew` length (13 chars) for best compatibility.

## Examples

```bash
# Standard init
zb init

# Use custom directories via env
ZEROBREW_ROOT=/custom/root ZEROBREW_PREFIX=/custom/prefix zb init

# Init without shell config edits
zb init --no-modify-path
```

{% call tip() %}
Most users do not need to run this manually; install and normal command flows run init when needed.
{% endcall %}
