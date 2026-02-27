---
layout: "layouts/docs-page.njk"
title: "completion"
description: "Generate shell completion scripts"
weight: 11
---

{% from "components/docs/shortcodes/macros.njk" import note, info, warning, tip, card_group, card, tabs, tab, accordion_group, accordion, steps, step, param_fields, param_field %}

## Usage

```bash
zb completion <shell>
```

## Description

Generates shell completion scripts for tab-completion of zerobrew commands.

## Arguments

{% call param_fields() %}
{% call param_field("shell", "string", "", true) %}
The shell to generate completions for. One of: `bash`, `zsh`, `fish`, `elvish`, `powershell`.
{% endcall %}
{% endcall %}

## Setup by Shell

{% call tabs() %}
{% call tab("zsh") %}
```bash
# Create completions directory if needed
mkdir -p ~/.zsh/completions

# Generate completions
zb completion zsh > ~/.zsh/completions/_zb

# Add to ~/.zshrc (if not already present)
echo 'fpath=(~/.zsh/completions $fpath)' >> ~/.zshrc
echo 'autoload -Uz compinit && compinit' >> ~/.zshrc

# Reload
source ~/.zshrc
```
{% endcall %}
{% call tab("bash") %}
```bash
# Create completions directory
mkdir -p ~/.local/share/bash-completion/completions

# Generate completions
zb completion bash > ~/.local/share/bash-completion/completions/zb

# Reload (or restart terminal)
source ~/.local/share/bash-completion/completions/zb
```
{% endcall %}
{% call tab("fish") %}
```bash
# Generate completions
zb completion fish > ~/.config/fish/completions/zb.fish

# Fish automatically loads completions from this directory
```
{% endcall %}
{% call tab("elvish") %}
```bash
# Generate and add to rc.elv
zb completion elvish >> ~/.elvish/rc.elv
```
{% endcall %}
{% call tab("PowerShell") %}
```powershell
# Generate and add to profile
zb completion powershell >> $PROFILE
```
{% endcall %}
{% endcall %}

## What Gets Completed

After setup, you can tab-complete:

- Commands: `zb ins<TAB>` → `zb install`
- Subcommands: `zb completion <TAB>` → shows `bash`, `zsh`, etc.
- Options: `zb install --<TAB>` → shows `--no-link`

## Automated Setup

You can also use the install script for completions:

```bash
./install-completions.sh
```

This detects your shell and installs completions automatically.
