---
layout: "layouts/docs-page.njk"
title: "info"
description: "Show install metadata for a formula"
weight: 6
---

{% from "components/docs/shortcodes/macros.njk" import note, info, warning, tip, card_group, card, tabs, tab, accordion_group, accordion, steps, step, param_fields, param_field %}

## Usage

```bash
zb info <formula>
```

## Description

Displays metadata for an installed formula. If formula is not installed, prints a not-installed message.

## Arguments

{% call param_fields() %}
{% call param_field("formula", "string", "", true) %}Formula to inspect.{% endcall %}
{% endcall %}

## Example

```bash
zb info sqlite
```

Example output:

```text
Name:       sqlite
Version:    3.50.4
Store key:  18f3d7e2a2bf
Installed:  2026-02-10 (2 days ago)
```

## Fields

| Field | Meaning |
|-------|---------|
| `Name` | Formula name |
| `Version` | Installed version |
| `Store key` | First 12 chars of content-addressed store hash |
| `Installed` | Local timestamp with relative age |
