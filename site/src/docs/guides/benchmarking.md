---
layout: "layouts/docs-page.njk"
title: "Benchmarking"
description: "Measure zerobrew performance against Homebrew"
weight: 2
---

{% from "components/docs/shortcodes/macros.njk" import note, info, warning, tip, card_group, card, tabs, tab, accordion_group, accordion, steps, step, param_fields, param_field %}

## Run benchmarks

Use the `Justfile` benchmark recipe:

```bash
# quick suite (default shortlist)
just bench --quick

# full suite (100 packages)
just bench --full
```

## Useful options

```bash
# limit number of tested packages
just bench --quick --count 10

# output format
just bench --full --format json --output results.json
just bench --full --format html --output results.html

# write all formats into a directory
just bench --full results/

# dry-run package selection
just bench --quick --dry-run
```

## What is measured

For each package, benchmark runs:

1. `brew install <pkg>`
2. `zb install <pkg>` (cold)
3. `zb install <pkg>` (warm)

Then it reports speedup ratios (`Homebrew / zerobrew`).

## Reading results

| Metric | Meaning |
|--------|---------|
| Cold | package not in store; download + install needed |
| Warm | package already in store; fast rematerialization |
| Speedup | Homebrew time divided by zerobrew time |

{% call info() %}
Cold results depend heavily on network and mirror conditions. Warm results are the best signal for repeated installs.
{% endcall %}

## Reproducibility tips

- run on a stable network
- avoid running other heavy downloads
- benchmark on a relatively idle machine
- use `--count` for repeatability while iterating

{% call warning() %}
The benchmark recipe performs uninstall/reset operations. Use a machine/environment where that is acceptable.
{% endcall %}
