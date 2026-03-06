---
layout: "layouts/docs-page.njk"
title: "run"
description: "Run a formula executable without linking it"
weight: 12
---

{% from "components/docs/shortcodes/macros.njk" import note, info, warning, tip, card_group, card, tabs, tab, accordion_group, accordion, steps, step, param_fields, param_field %}

## Usage

```bash
zb run <formula> [args...]
```

## Description

Ensures a formula is installed, then executes its binary directly from Cellar.

- if formula is not installed, zerobrew installs it first
- execution does **not** require creating `prefix/bin` symlinks
- runtime cert and library env vars are set from `ZEROBREW_PREFIX` when available

## Arguments

{% call param_fields() %}
{% call param_field("formula", "string", "", true) %}Formula executable to run.{% endcall %}
{% endcall %}

## Examples

```bash
zb run jq --version
zb run wget https://example.com
```

## `zbx` shortcut

`zbx` is a thin wrapper around `zb run`:

```bash
zbx jq --version
# equivalent to:
zb run jq --version
```

{% call note() %}
`run` exists as a command in `zb --help`, but most users should prefer `zbx` for one-off execution.
{% endcall %}
