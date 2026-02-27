---
layout: "layouts/docs-page.njk"
title: "uninstall"
description: "Remove installed formulas"
weight: 4
---

{% from "components/docs/shortcodes/macros.njk" import note, info, warning, tip, card_group, card, tabs, tab, accordion_group, accordion, steps, step, param_fields, param_field %}

## Usage

```bash
zb uninstall [OPTIONS] <formula>...
zb uninstall --all
```

## Description

Uninstalls formulas from Cellar and removes their links from prefix directories.
Store entries remain cached for fast reinstall.

## Options

{% call param_fields() %}
{% call param_field("--all", "boolean", "", false) %}Uninstall all currently installed formulas.{% endcall %}
{% endcall %}

## Arguments

{% call param_fields() %}
{% call param_field("formula", "string", "", false) %}One or more formula names. Required unless `--all` is used.{% endcall %}
{% endcall %}

## Examples

```bash
# Uninstall one formula
zb uninstall jq

# Uninstall multiple formulas
zb uninstall jq wget git

# Uninstall everything currently installed
zb uninstall --all
```

## What gets removed

| Removed | Kept |
|---------|------|
| `prefix/Cellar/<formula>` entries | Store blobs in `store/` |
| symlinks in `prefix/bin`, `lib`, etc. | Download cache |
| `prefix/opt/<formula>` symlink | other installed formulas |

{% call info() %}
Use `zb gc` to remove unreferenced store entries after uninstalling.
{% endcall %}
