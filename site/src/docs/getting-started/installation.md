---
layout: "layouts/docs-page.njk"
title: "Installation"
description: "Detailed installation options for zerobrew"
weight: 3
---

{% from "components/docs/shortcodes/macros.njk" import note, info, warning, tip, card_group, card, tabs, tab, accordion_group, accordion, steps, step, param_fields, param_field %}

## Quick Install

The easiest way to install zerobrew:

```bash
curl -sSL https://raw.githubusercontent.com/lucasgelfond/zerobrew/main/install.sh | bash
```

{% call warning() %}
Do not run the install script with `sudo`. The script handles privilege escalation automatically when needed.
{% endcall %}

## What the Installer Does

1. **Checks for Rust** — installs via rustup if not found
2. **Clones the repository** to `~/.zerobrew`
3. **Builds from source** with `cargo build --release`
4. **Installs binaries** to `~/.local/bin/` (`zb` and `zbx`)
5. **Creates data directories** at `/opt/zerobrew`
6. **Updates your shell config** to add zerobrew to PATH

## Build from Source

If you prefer to build manually:

```bash
# Clone the repository
git clone https://github.com/lucasgelfond/zerobrew.git
cd zerobrew

# Build release binary
cargo build --release

# Install to your PATH
cargo install --path zb_cli
```

### Prerequisites

- Rust 1.90 or later
- macOS (Apple Silicon or Intel) or Linux

## Custom Installation Paths

zerobrew respects several environment variables for custom installation:

{% call param_fields() %}
{% call param_field("ZEROBREW_ROOT", "string", "/opt/zerobrew", false) %}
Data directory containing store, database, cache, and prefix.
{% endcall %}

{% call param_field("ZEROBREW_PREFIX", "string", "$ZEROBREW_ROOT/prefix", false) %}
Installation prefix for symlinked executables and libraries.
{% endcall %}

{% call param_field("ZEROBREW_DIR", "string", "~/.zerobrew", false) %}
Source code directory.
{% endcall %}

{% call param_field("ZEROBREW_BIN", "string", "~/.local/bin", false) %}
Location for the `zb` binary.
{% endcall %}
{% endcall %}

### Example: Custom Paths

```bash
export ZEROBREW_ROOT="$HOME/.local/share/zerobrew"
export ZEROBREW_PREFIX="$HOME/.local"
curl -sSL https://raw.githubusercontent.com/lucasgelfond/zerobrew/main/install.sh | bash
```

## Post-Installation

After installation, add zerobrew to your PATH:

{% call accordion_group() %}
{% call accordion("zsh") %}
The installer automatically adds to `~/.zshrc` or `~/.zshenv`. To apply immediately:

```bash
export PATH="$HOME/.local/bin:$ZEROBREW_PREFIX/bin:$PATH"
```
{% endcall %}
{% call accordion("bash") %}
The installer automatically adds to `~/.bashrc` or `~/.bash_profile`. To apply immediately:

```bash
export PATH="$HOME/.local/bin:$ZEROBREW_PREFIX/bin:$PATH"
```
{% endcall %}
{% endcall %}

Or simply restart your terminal.

## Verify Installation

```bash
zb --version
```

You should see output like:

```
zb 0.1.0
```

## Shell Completions

Enable tab completion for your shell:

{% call tabs() %}
{% call tab("zsh") %}
```bash
zb completion zsh > ~/.zsh/completions/_zb
```
{% endcall %}
{% call tab("bash") %}
```bash
zb completion bash > ~/.local/share/bash-completion/completions/zb
```
{% endcall %}
{% call tab("fish") %}
```bash
zb completion fish > ~/.config/fish/completions/zb.fish
```
{% endcall %}
{% endcall %}

## Uninstalling

To completely remove zerobrew:

```bash
# Reset all installed packages
zb reset -y

# Remove zerobrew directories
rm -rf ~/.zerobrew
sudo rm -rf /opt/zerobrew
rm ~/.local/bin/zb ~/.local/bin/zbx

# Remove from shell config (manually edit ~/.zshrc or ~/.bashrc)
```

Or use the Justfile command:

```bash
just uninstall
```
