+++
title = "Installation"
description = "Detailed installation options for zerobrew"
weight = 3
+++

## Quick Install

The easiest way to install zerobrew:

```bash
curl -sSL https://raw.githubusercontent.com/lucasgelfond/zerobrew/main/install.sh | bash
```

{% warning() %}
Do not run the install script with `sudo`. The script handles privilege escalation automatically when needed.
{% end %}

## What the Installer Does

1. **Checks for Rust** — installs via rustup if not found
2. **Clones the repository** to `~/.zerobrew`
3. **Builds from source** with `cargo build --release`
4. **Installs the binary** to `~/.local/bin/zb`
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

{% param_field(path="ZEROBREW_ROOT", type="string", default="/opt/zerobrew") %}
Data directory containing store, database, cache, and prefix.
{% end %}

{% param_field(path="ZEROBREW_PREFIX", type="string", default="$ZEROBREW_ROOT/prefix") %}
Installation prefix for symlinked executables and libraries.
{% end %}

{% param_field(path="ZEROBREW_DIR", type="string", default="~/.zerobrew") %}
Source code directory.
{% end %}

{% param_field(path="ZEROBREW_BIN", type="string", default="~/.local/bin") %}
Location for the `zb` binary.
{% end %}

### Example: Custom Paths

```bash
export ZEROBREW_ROOT="$HOME/.local/share/zerobrew"
export ZEROBREW_PREFIX="$HOME/.local"
curl -sSL https://raw.githubusercontent.com/lucasgelfond/zerobrew/main/install.sh | bash
```

## Post-Installation

After installation, add zerobrew to your PATH:

{% accordion_group() %}
{% accordion(title="zsh") %}
The installer automatically adds to `~/.zshrc` or `~/.zshenv`. To apply immediately:

```bash
export PATH="$HOME/.local/bin:/opt/zerobrew/prefix/bin:$PATH"
```
{% end %}
{% accordion(title="bash") %}
The installer automatically adds to `~/.bashrc` or `~/.bash_profile`. To apply immediately:

```bash
export PATH="$HOME/.local/bin:/opt/zerobrew/prefix/bin:$PATH"
```
{% end %}
{% end %}

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

{% tabs() %}
{% tab(title="zsh") %}
```bash
zb completion zsh > ~/.zsh/completions/_zb
```
{% end %}
{% tab(title="bash") %}
```bash
zb completion bash > ~/.local/share/bash-completion/completions/zb
```
{% end %}
{% tab(title="fish") %}
```bash
zb completion fish > ~/.config/fish/completions/zb.fish
```
{% end %}
{% end %}

## Uninstalling

To completely remove zerobrew:

```bash
# Reset all installed packages
zb reset -y

# Remove zerobrew directories
rm -rf ~/.zerobrew
sudo rm -rf /opt/zerobrew
rm ~/.local/bin/zb

# Remove from shell config (manually edit ~/.zshrc or ~/.bashrc)
```

Or use the Justfile command:

```bash
just uninstall
```
