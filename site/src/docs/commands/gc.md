---
layout: "layouts/docs-page.njk"
title: "gc"
description: "Garbage collect unreferenced store entries"
weight: 8
---

{% from "components/docs/shortcodes/macros.njk" import note, info, warning, tip, card_group, card, tabs, tab, accordion_group, accordion, steps, step, param_fields, param_field %}

## Usage

```bash
zb gc
```

## Description

Removes content-addressed store entries that are no longer referenced by installed formulas.

## Example

```bash
zb gc
```

Typical output:

```text
==> Running garbage collection...
    ✓ Removed 7a9b3d6f21ad
    ✓ Removed c1f4e0d92b8e
==> Removed 2 store entries
```

If nothing can be collected:

```text
No unreferenced store entries to remove.
```

{% call tip() %}
Use `zb gc` after uninstalling many formulas to reclaim disk space.
{% endcall %}
