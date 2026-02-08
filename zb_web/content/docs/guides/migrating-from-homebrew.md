+++
title = "Migrating from Homebrew"
description = "Move your existing packages to zerobrew"
weight = 1
+++

## Overview

If you're already using Homebrew, zerobrew can migrate your packages automatically. This guide covers the migration process and what to expect.

## Quick Migration

```bash
zb migrate
```

This will:
1. Detect all your Homebrew formulas
2. Install them via zerobrew
3. Optionally uninstall them from Homebrew

## What Can Be Migrated

| Package Type | Supported |
|-------------|-----------|
| Core formulas (git, wget, ffmpeg) | ✓ Yes |
| Formulas with bottles | ✓ Yes |
| Custom taps | ✗ No |
| Casks (GUI apps) | ✗ No |
| Source-only formulas | ✗ No |

{% info() %}
zerobrew only supports packages with pre-built bottles from Homebrew's core tap.
{% end %}

## Step-by-Step Migration

### 1. Check What's Installed

First, see what Homebrew has:

```bash
brew list --formula
brew list --cask
```

### 2. Run Migration

```bash
zb migrate
```

zerobrew will show you:
- Core formulas that will be migrated
- Non-core formulas that can't be migrated
- Casks that will remain in Homebrew

### 3. Confirm Migration

Review the list and confirm:

```
The following 25 formulas will be migrated:
    • git
    • wget
    • jq
    ...

Continue with migration? [y/N] y
```

### 4. Handle Failures

If any packages fail:

```bash
# Check what failed
# Retry manually
zb install <failed-package>

# Or use Homebrew for unsupported packages
brew install <package>
```

### 5. Optionally Remove from Homebrew

After migration succeeds:

```
Uninstall 25 formula(s) from Homebrew? [y/N] y
```

## Keeping Both

You don't have to choose — Homebrew and zerobrew can coexist:

```bash
# zerobrew for supported packages
zb install git wget jq ffmpeg

# Homebrew for taps and casks
brew install --cask docker visual-studio-code
brew install custom/tap/special-package
```

Both use different directories:
- zerobrew: `/opt/zerobrew/prefix/`
- Homebrew: `/opt/homebrew/` (Apple Silicon) or `/usr/local/` (Intel)

{% warning() %}
If you have both in PATH, zerobrew's bin should come first for packages you want zerobrew to manage.
{% end %}

## PATH Order

Ensure zerobrew's bin comes before Homebrew's in your PATH:

```bash
# Good: zerobrew first
export PATH="/opt/zerobrew/prefix/bin:/opt/homebrew/bin:$PATH"

# Verify
which git  # Should show /opt/zerobrew/prefix/bin/git
```

## Rollback

If you need to go back to Homebrew:

```bash
# Reset zerobrew
zb reset -y

# Reinstall via Homebrew
brew install git wget jq ffmpeg
```

## Disk Space

After migration, you can reclaim Homebrew's disk space:

```bash
# After confirming zerobrew works
brew cleanup --prune=all

# Or remove Homebrew entirely (careful!)
/bin/bash -c "$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/HEAD/uninstall.sh)"
```

{% tip() %}
Keep Homebrew installed for casks and taps, just clean up formulas you've migrated.
{% end %}
