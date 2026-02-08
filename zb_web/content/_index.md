+++
title = ""

[extra.hero]
title = "_Zerobrew_ is a drop-in replacement<br> for Homebrew, achieving **20x/5x**<br> the speed with the same packages."

[extra.installs]
tabs = [
{ name = "bash", command="curl -fsSL https://zerobrew.rs/install | bash" },
{ name = "brew", command="brew install zerobrew" },
{ name = "aur", command="yay -S zerobrew-git" },
]
install_paragraph = "Install zerobrew in seconds and keep your existing Homebrew formulas. No rewrites, just faster installs." 

[extra.bench]
benchmarks = [
    { title = "Total install time (10 packages)", items = [
        { label = "ZB", tag = "warm", value = 4, time = "15s", speedup = "28.6x", color = "var(--accent)" },
        { label = "ZB", tag = "cold", value = 14, time = "62s", speedup = "7.0x", color = "var(--bench-cold)" },
        { label = "HB", tag = "", value = 100, time = "432s", color = "var(--bench-hb)" },
    ] },
]
+++
## Why zerobrew

zerobrew is a drop-in, CLI-first replacement for Homebrew that focuses on speed, determinism, and a clean store layout. It keeps the Homebrew formula ecosystem you already rely on, but avoids re-downloading and re-linking the same bits over and over.

{% card_group(cols=3) %}
{% card(title="Content-addressable store", href="/docs/architecture/") %}
Packages are stored by hash, so re-installs are instant and deduplicated.
{% end %}
{% card(title="Parallel pipeline", href="/docs/architecture/") %}
Downloads, extraction, and linking run concurrently for dramatic speedups.
{% end %}
{% card(title="Homebrew compatible", href="/docs/commands/overview/") %}
Use the same formula names and swap `brew` for `zb`.
{% end %}
{% end %}

## What you can do today

- Install core Homebrew formulas with `zb install`.
- Migrate an existing Homebrew setup with `zb migrate`.
- Clean up store space with `zb gc`.

## Get started

Head to the docs for a fast setup path, then dive into configuration and command references.

{% card_group(cols=2) %}
{% card(title="Quickstart", href="/docs/quickstart/") %}
Install zerobrew in under a minute.
{% end %}
{% card(title="Installation", href="/docs/installation/") %}
Detailed setup steps and shell configuration.
{% end %}
{% end %}
