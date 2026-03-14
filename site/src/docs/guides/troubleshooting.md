---
layout: "layouts/docs-page.njk"
title: "Troubleshooting"
description: "Common issues and how to resolve them"
weight: 3
---

{% from "components/docs/shortcodes/macros.njk" import note, info, warning, tip, card_group, card, tabs, tab, accordion_group, accordion, steps, step, param_fields, param_field %}

## Installation Issues

### "Command not found: zb"

Your PATH doesn't include zerobrew's bin directory.

**Fix:**
```bash
export PATH="$HOME/.local/bin:$ZEROBREW_PREFIX/bin:$PATH"
```

Or restart your terminal to load the updated shell config.

### "Permission denied" during install

The installer needs to create `/opt/zerobrew` which requires sudo.

**Fix:**
```bash
# Don't run the whole script with sudo
# Just let it prompt for sudo when needed
curl -sSL https://raw.githubusercontent.com/lucasgelfond/zerobrew/main/install.sh | bash
```

If `/opt/zerobrew` already exists with wrong permissions:
```bash
sudo chown -R $(whoami) /opt/zerobrew
```

### Rust/Cargo not found

The installer should handle this, but if it fails:

**Fix:**
```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source $HOME/.cargo/env
```

## Package Issues

### "Formula not found"

zerobrew only supports core Homebrew formulas with bottles.

**Causes:**
- Package is from a tap (e.g., `homebrew/services`)
- Package is source-only (no bottle)
- Package name is misspelled

**Fix:**
```bash
# Check if it exists in Homebrew
brew info <package>

# Use Homebrew for unsupported packages
brew install <package>
```

### Package installs but command not found

The package might not have been linked.

**Fix:**
```bash
# Check if it's installed
zb list

# Verify PATH
echo $PATH | tr ':' '\n' | grep zerobrew

# Make sure zerobrew's bin is in PATH
export PATH="$ZEROBREW_PREFIX/bin:$PATH"
```

### Conflicting versions with Homebrew

If you have both zerobrew and Homebrew, ensure zerobrew's PATH comes first.

**Fix:**
```bash
# In ~/.zshrc or ~/.bashrc
export PATH="$ZEROBREW_PREFIX/bin:/opt/homebrew/bin:$PATH"

# Verify
which git  # Should show zerobrew path
```

## Performance Issues

### Slow downloads

Try reducing concurrency if you're on a slow connection:

```bash
zb --concurrency 8 install <package>
```

### Warm installs not instant

The store might be corrupted or on a non-APFS filesystem.

**Check:**
```bash
# Verify filesystem
diskutil info / | grep "File System"
# Should show "APFS"

# Check store integrity
ls -la "$ZEROBREW_ROOT/store/"
```

**Fix:**
```bash
# Reset and reinstall
zb reset -y
zb install <packages>
```

## Database Issues

### "Database locked" or corruption errors

**Fix:**
```bash
# Stop any running zb processes
pkill -f zb

# Remove lock files
rm -rf "$ZEROBREW_ROOT/locks"/*

# If still failing, reset database
rm -rf "$ZEROBREW_ROOT/db"/*
zb init
```

## Getting Help

### Check Logs

zerobrew prints detailed progress during operations. For more info:

```bash
# Run with RUST_LOG for debug output
RUST_LOG=debug zb install <package>
```

### Report Issues

If you've found a bug:

1. Check [existing issues](https://github.com/lucasgelfond/zerobrew/issues)
2. Include: macOS version, error message, steps to reproduce
3. Open a new issue with this info

### Community Support

Join the [Discord](https://discord.gg/ZaPYwm9zaw) for help from the community and maintainers.
