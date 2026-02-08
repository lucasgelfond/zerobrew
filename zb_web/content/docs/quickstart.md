+++
title = "Quickstart"
description = "Get zerobrew installed in under a minute"
weight = 2
+++

## Install zerobrew

Run the install script:

```bash
curl -sSL https://raw.githubusercontent.com/lucasgelfond/zerobrew/main/install.sh | bash
```

After install, either run the export command it prints, or restart your terminal.

{% tip() %}
The installer will automatically install Rust if you don't have it.
{% end %}

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

{% info() %}
Learn more in the [Migration Guide](/docs/guides/migrating-from-homebrew/).
{% end %}

## Next Steps

{% card_group(cols=2) %}
{% card(title="All Commands", icon="terminal", href="/docs/commands/overview/") %}
Explore the full command reference.
{% end %}
{% card(title="Configuration", icon="gear", href="/docs/configuration/") %}
Customize zerobrew's behavior.
{% end %}
{% card(title="Architecture", icon="sitemap", href="/docs/architecture/") %}
Learn how zerobrew works.
{% end %}
{% card(title="Contributing", icon="code", href="/docs/contributing/") %}
Help improve zerobrew.
{% end %}
{% end %}
