---
layout: "layouts/docs-page.njk"
title: "list"
description: "Show installed formulas"
weight: 5
---

{% from "components/docs/shortcodes/macros.njk" import note, info, warning, tip, card_group, card, tabs, tab, accordion_group, accordion, steps, step, param_fields, param_field %}

## Usage

```bash
zb list
```

## Description

Lists currently installed formulas with version numbers.

## Example

```bash
zb list
```

Example output:

```text
git 2.51.0
jq 1.8.1
sqlite 3.50.4
```

If nothing is installed:

```text
No formulas installed.
```
