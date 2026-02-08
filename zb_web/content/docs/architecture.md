+++
title = "Architecture"
description = "How zerobrew achieves its speed"
weight = 1
+++

## Overview

zerobrew is designed around a simple principle: **never download or install the same thing twice**.

It achieves this through a content-addressable store, where packages are indexed by their SHA-256 hash. Combined with parallel downloads, streaming extraction, and APFS copy-on-write, this leads to dramatic performance improvements.

## Storage Layout

```
/opt/zerobrew/              # Data directory ($ZEROBREW_ROOT)
├── store/                  # sha256-addressable packages
├── db/                     # SQLite database
├── cache/                  # Downloaded bottle blobs
├── locks/                  # Per-entry file locks
└── prefix/                 # $ZEROBREW_PREFIX
    ├── bin/                # Symlinked executables
    ├── Cellar/             # Materialized packages
    ├── lib/
    ├── include/
    ├── share/
    └── opt/                # Symlinked package directories

~/.zerobrew/                # Source code ($ZEROBREW_DIR)
~/.local/bin/zb             # Binary ($ZEROBREW_BIN)
```

## Content-Addressable Store

When you install a package like `sqlite`:

1. zerobrew fetches the formula from Homebrew's API
2. Downloads the bottle (pre-compiled binary) from Homebrew's CDN
3. Computes the SHA-256 hash of the bottle
4. Stores the extracted contents at `/opt/zerobrew/store/{sha256}/`

### Why This Matters

- **Reinstalls are instant** — if the hash exists, no download needed
- **No version conflicts** — different versions have different hashes
- **Deduplication** — identical packages only stored once

## The Installation Pipeline

zerobrew uses a streaming pipeline where each stage runs concurrently:

{% card_group(cols=4) %}
{% card(title="1. Resolve", icon="resolve") %}
Dependencies
{% end %}
{% card(title="2. Download", icon="download") %}
Parallel
{% end %}
{% card(title="3. Extract", icon="extract") %}
Streaming
{% end %}
{% card(title="4. Link", icon="link") %}
Clonefile
{% end %}
{% end %}

{% steps() %}
{% step(title="Resolve Dependencies") %}
Fetches formula metadata from Homebrew's API and builds a dependency graph.
{% end %}
{% step(title="Download (Parallel)") %}
Downloads all bottles in parallel, deduplicating in-flight requests and racing across CDN connections.
{% end %}
{% step(title="Extract (Streaming)") %}
Extracts tar.gz archives as bytes arrive — no waiting for complete download.
{% end %}
{% step(title="Link (Clonefile)") %}
Uses APFS `clonefile()` for zero-copy materialization, then creates symlinks.
{% end %}
{% end %}

## APFS Clonefile

On macOS, zerobrew uses `clonefile()` to materialize packages from the store:

```rust
// Pseudo-code
clonefile(
    "/opt/zerobrew/store/{sha256}/sqlite/3.45.0",
    "/opt/zerobrew/prefix/Cellar/sqlite/3.45.0"
)
```

This is **instant** and uses **zero additional disk space** until files are modified (copy-on-write).

{% info() %}
This is the same technology that makes duplicating large files in Finder instant.
{% end %}

## Database

zerobrew maintains a SQLite database at `/opt/zerobrew/db/` that tracks:

- Installed packages and their versions
- Store entries and their hashes
- Dependency relationships
- Installation timestamps

This enables fast lookups without scanning the filesystem.

## Project Structure

zerobrew is organized as a Cargo workspace with three crates:

| Crate | Purpose |
|-------|---------|
| `zb_core` | Core data models and domain logic (formula resolution, bottle selection) |
| `zb_io` | I/O operations (API client, downloads, extraction, installation) |
| `zb_cli` | Command-line interface |

```
zerobrew/
├── zb_core/          # Core logic
│   └── src/
│       ├── bottle.rs
│       ├── context.rs
│       ├── errors.rs
│       ├── formula.rs
│       └── resolve.rs
├── zb_io/            # I/O operations
│   └── src/
│       ├── api.rs
│       ├── download.rs
│       ├── extract.rs
│       ├── install.rs
│       ├── link.rs
│       └── store.rs
└── zb_cli/           # CLI
    └── src/
        ├── cli.rs
        ├── main.rs
        └── commands/
```

## Compatibility with Homebrew

zerobrew uses Homebrew's:

- **Formula API** — fetches package metadata from `formulae.brew.sh`
- **Bottle CDN** — downloads pre-compiled binaries from `ghcr.io`
- **Formula names** — use the same names as `brew install`

This means you can swap `brew` for `zb` in most cases:

```bash
# These are equivalent
brew install jq
zb install jq
```

{% warning() %}
zerobrew only supports **core formulas** (bottles). Casks, taps, and source-only formulas are not supported.
{% end %}
