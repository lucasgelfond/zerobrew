+++
title = "Documentation"
description = "Everything you need to install, configure, and use zerobrew."
template = "docs.html"
page_template = "docs-page.html"
sort_by = "weight"

[extra]
nav = [
  { title = "Get Started", pages = ["docs/introduction.md", "docs/quickstart.md", "docs/installation.md"] },
  { title = "Core Concepts", pages = ["docs/architecture.md", "docs/configuration.md"] },
  { title = "Commands", pages = [
    "docs/commands/overview.md",
    "docs/commands/install.md",
    "docs/commands/uninstall.md",
    "docs/commands/list.md",
    "docs/commands/info.md",
    "docs/commands/migrate.md",
    "docs/commands/gc.md",
    "docs/commands/reset.md",
    "docs/commands/init.md",
    "docs/commands/completion.md"
  ] },
  { title = "Guides", pages = [
    "docs/guides/migrating-from-homebrew.md",
    "docs/guides/benchmarking.md",
    "docs/guides/troubleshooting.md"
  ] },
  { title = "Community", pages = ["docs/contributing.md"] }
]
+++

## Start here

If you're new, read the introduction, then follow Quickstart. Installation covers deeper setup and shell configuration.

## How to navigate

- **Get Started**: high-level overview and your first install.
- **Core Concepts**: architecture and configuration details.
- **Commands**: full CLI reference by command.
- **Guides**: migration, benchmarking, and troubleshooting.
- **Community**: contributing and workflow expectations.

{% note() %}
If you are coming from Homebrew, check the migration guide after Quickstart.
{% end %}
