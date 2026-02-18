---
layout: "layouts/docs-page.njk"
title: "reset"
description: "Clear zerobrew root/prefix contents and re-initialize"
weight: 9
---

{% from "components/docs/shortcodes/macros.njk" import note, info, warning, tip, card_group, card, tabs, tab, accordion_group, accordion, steps, step, param_fields, param_field %}

## Usage

```bash
zb reset [OPTIONS]
```

## Description

Clears contents of `ZEROBREW_ROOT` and `ZEROBREW_PREFIX`, then runs `zb init` again.
This gives you a clean state for cold installs.

## Options

{% call param_fields() %}
{% call param_field("-y, --yes", "boolean", "", false) %}Skip confirmation prompt.{% endcall %}
{% endcall %}

## What reset does

1. Prompts before destructive changes (unless `-y`)
2. Clears contents under root and prefix
3. Falls back to `sudo rm -rf` when needed
4. Re-runs initialization

## Example

```bash
zb reset -y
```

Typical output:

```text
==> Clearing /opt/zerobrew...
==> Clearing /opt/zerobrew/prefix...
==> Initializing zerobrew...
==> Initialization complete!
==> Reset complete. Ready for cold install.
```

{% call warning() %}
`zb reset` is destructive for all zerobrew-managed installs and cache data.
{% endcall %}

## Full uninstall

If you want to remove zerobrew entirely (binary + source + data + shell config), use:

```bash
just uninstall
```
