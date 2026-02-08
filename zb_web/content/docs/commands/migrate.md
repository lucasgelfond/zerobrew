+++
title = "migrate"
description = "Migrate packages from Homebrew to zerobrew"
weight = 6
+++

## Usage

```bash
zb migrate [OPTIONS]
```

## Description

Detects all packages installed via Homebrew and migrates them to zerobrew. Optionally uninstalls the packages from Homebrew after successful migration.

## Options

{% param_field(path="-y, --yes", type="boolean") %}
Skip confirmation prompts. Useful for scripting.
{% end %}

{% param_field(path="--force", type="boolean") %}
Force uninstall from Homebrew even if there are dependents.
{% end %}

## Example

```bash
zb migrate
```

Output:
```
==> Fetching installed Homebrew packages...
    15 core formulas, 2 non-core formulas, 3 casks found

Note: Formulas from non-core taps cannot be migrated to zerobrew:
    • some-package (homebrew/services)
    • another-pkg (custom/tap)

Note: Casks cannot be migrated to zerobrew (only CLI formulas are supported):
    • visual-studio-code
    • docker
    • slack

The following 15 formulas will be migrated:
    • git
    • wget
    • jq
    • ffmpeg
    ...

Continue with migration? [y/N] y

==> Migrating 15 formulas to zerobrew...
    ○ git... ✓
    ○ wget... ✓
    ○ jq... ✓
    ...

==> Migrated 15 of 15 formulas to zerobrew

Uninstall 15 formula(s) from Homebrew? [y/N] y

==> Uninstalling from Homebrew...
    ○ git... ✓
    ○ wget... ✓
    ...

==> Uninstalled 15 of 15 formula(s) from Homebrew
```

## What Gets Migrated

| Type | Migrated? |
|------|-----------|
| Core formulas (e.g., `git`, `wget`) | ✓ Yes |
| Non-core taps | ✗ No |
| Casks (GUI apps) | ✗ No |
| Source-only formulas | ✗ No |

{% warning() %}
zerobrew only supports core Homebrew formulas with pre-built bottles. Packages from custom taps or casks must remain in Homebrew.
{% end %}

## Migration Workflow

{% steps() %}
{% step(title="Detect") %}
Runs `brew list` to find all installed formulas and casks.
{% end %}
{% step(title="Categorize") %}
Separates core formulas, tap formulas, and casks.
{% end %}
{% step(title="Confirm") %}
Shows what will be migrated and asks for confirmation.
{% end %}
{% step(title="Install") %}
Installs each formula via zerobrew.
{% end %}
{% step(title="Uninstall") %}
Optionally removes from Homebrew after successful install.
{% end %}
{% end %}

## Non-Interactive Mode

For scripting or automation:

```bash
zb migrate -y
```

This skips all confirmation prompts and proceeds with migration and uninstallation.

## Handling Failures

If some packages fail to migrate:

- zerobrew reports which packages failed
- Failed packages are skipped during Homebrew uninstall
- You can retry individual packages with `zb install <package>`

```
==> Migrated 14 of 15 formulas to zerobrew
Warning: Failed to migrate 1 formula(s):
    • problematic-package

# Retry manually
zb install problematic-package
```

## Tips

{% tip() %}
Run `zb migrate` without `-y` first to see what will be migrated before committing.
{% end %}

{% info() %}
You can keep both Homebrew and zerobrew installed. They use different directories and don't conflict.
{% end %}
