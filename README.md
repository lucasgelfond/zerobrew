# zerobrew

a faster homebrew. inspired by [uv](https://github.com/astral-sh/uv).

## what is this?

zerobrew is a drop-in replacement for `brew install` that's faster, especially on warm installs. it downloads prebuilt bottles from homebrew's CDN and installs them using content-addressable storage.

```bash
zb install jq        # install jq
zb install wget git  # install multiple
zb uninstall jq      # uninstall
zb uninstall         # uninstall everything
```

## why is it faster?

### 1. content-addressable store

instead of extracting bottles directly to the cellar like homebrew does, zerobrew uses a two-tier storage model:

```
/opt/zerobrew/store/{sha256}/    # deduped, content-addressable
/opt/homebrew/Cellar/{name}/     # materialized via clonefile
```

the store is keyed by sha256. if you install something, uninstall it, and reinstall it - the second install is instant because the store entry still exists.

### 2. apfs clonefile

when materializing from store to cellar, zerobrew uses apfs clonefile (copy-on-write). this means the "copy" is instant and uses zero additional disk space until files are modified.

fallback chain: clonefile → hardlink → copy

### 3. parallel downloads with deduplication

if package A and package B both depend on openssl@3, zerobrew only downloads it once. the parallel downloader deduplicates in-flight requests using broadcast channels.

### 4. api response caching

formula metadata is cached with etag support. on subsequent installs, zerobrew sends `If-None-Match` and gets a 304 response - no body to parse.

### 5. deterministic resolution

homebrew's resolver can produce different installation orders across runs. zerobrew uses a proper topological sort with BTreeSet for stable, deterministic ordering. this matters for caching - same inputs = same outputs.

## how it differs from homebrew

| | homebrew | zerobrew |
|-|----------|----------|
| storage | direct to cellar | store (deduped) + cellar |
| copy strategy | full copy | clonefile/hardlink |
| downloads | sequential | parallel with dedup |
| api calls | fetch every time | cached with etag |
| resolution | dynamic | topological sort |
| reinstall | re-download, re-extract | instant from store |

## what's the same

- uses homebrew's bottle CDN (ghcr.io/v2/homebrew/core)
- uses homebrew's formula API (formulae.brew.sh)
- installs to /opt/homebrew (compatible with homebrew)
- arm64 macos only (for now)

## design decisions

**skip homebrew-installed packages**: if a formula is already in /opt/homebrew/Cellar, zerobrew skips it. this lets you use both tools together without conflicts.

**reference counting for gc**: each store entry tracks how many installed packages reference it. uninstall decrements the count, `gc` removes entries with zero refs.

**sqlite for state**: installed packages are tracked in sqlite, not filesystem scanning. this makes queries fast and atomic.

**code signing**: bottles are ad-hoc signed after extraction to avoid macos killing unsigned binaries.

## inspired by uv

[uv](https://github.com/astral-sh/uv) showed that package managers can be 10-100x faster with:
- content-addressable storage
- parallel operations
- aggressive caching
- rust

zerobrew applies the same principles to homebrew.

## build

```bash
cargo build --release
cargo install --path zb_cli
```

## status

experimental. works for simple packages. some formulas need more work (virtual packages, tap formulas, casks).
