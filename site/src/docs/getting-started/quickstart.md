---
layout: "layouts/docs-page.njk"
title: "Quickstart"
description: "Get zerobrew installed in under a minute"
weight: 2
---

{% from "components/docs/shortcodes/macros.njk" import note, info, warning, tip, card_group, card, tabs, tab, accordion_group, accordion, steps, step, param_fields, param_field %}

## Install zerobrew

Run the install script:

```bash
curl -sSL https://raw.githubusercontent.com/lucasgelfond/zerobrew/main/install.sh | bash
```

After install, either run the export command it prints, or restart your terminal.

{% call tip() %}
The installer will automatically install Rust if you don't have it.
{% endcall %}

## Your First Package

Install a package:

```bash
zb install jq
```

That's it! You'll see zerobrew resolve dependencies, download, and link the package.

## Common Commands

| Command | Description |
|---------|-------------|
| `zb install <package>` | Install a package |
| `zb uninstall <package>` | Remove a package |
| `zb list` | Show installed packages |
| `zb info <package>` | Display package details |

## Install Multiple Packages

You can install several packages at once:

```bash
zb install wget git ffmpeg sqlite
```

zerobrew will resolve all dependencies and install everything in parallel.

## Migrating from Homebrew

If you have existing Homebrew packages, migrate them with one command:

```bash
zb migrate
```

This will:
1. Detect all your installed Homebrew formulas
2. Install them via zerobrew
3. Optionally uninstall them from Homebrew

{% call info() %}
Learn more in the [Migration Guide](/docs/guides/migrating-from-homebrew/).
{% endcall %}

## Next Steps

{% call card_group(2) %}
{% call card("All Commands", "/docs/commands/", "terminal") %}
Explore the full command reference.
{% endcall %}
{% call card("Configuration", "/docs/general/configuration/", "gear") %}
Customize zerobrew's behavior.
{% endcall %}
{% call card("Architecture", "/docs/general/architecture/", "sitemap") %}
Learn how zerobrew works.
{% endcall %}
{% call card("Contributing", "/docs/general/contributing/", "code") %}
Help improve zerobrew.
{% endcall %}
{% endcall %}
