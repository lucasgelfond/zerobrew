---
layout: "layouts/docs-page.njk"
title: "bundle"
description: "Install formulas from a Brewfile-style manifest"
weight: 3
---

{% from "components/docs/shortcodes/macros.njk" import note, info, warning, tip, card_group, card, tabs, tab, accordion_group, accordion, steps, step, param_fields, param_field %}

## Usage

```bash
zb bundle [OPTIONS]
```

## Description

Reads a manifest file (default: `Brewfile`) and installs formulas listed in it.

- blank lines are ignored
- lines starting with `#` are ignored
- inline comments are supported
- duplicate entries are deduplicated

## Options

{% call param_fields() %}
{% call param_field("-f, --file", "path", "Brewfile", false) %}Select manifest file.{% endcall %}
{% call param_field("--no-link", "boolean", "", false) %}Install without linking into prefix/bin.{% endcall %}
{% endcall %}

## Example Manifest

```text
# Brewfile
jq
wget
git
```

## Examples

```bash
# Use default ./Brewfile
zb bundle

# Use a custom file
zb bundle --file ./dev.Brewfile

# Install only into store/cellar
zb bundle --no-link
```

{% call tip() %}
`zb bundle` installs formulas sequentially from the manifest. Dependency download/extract/link work is still parallelized inside each install.
{% endcall %}
