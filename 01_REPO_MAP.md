# zerobrew — Repository Map

## Project Architecture

```
zerobrew (Cargo workspace)
├── zb_core     — Domain logic, formula resolution, bottle selection
├── zb_io       — I/O: API client, downloads, extraction, installation
└── zb_cli      — CLI commands: install/uninstall/bundle/migrate/doctor/gc/reset/init/completion/run/update/outdated
```

## Crate Breakdown

### zb_core (`/root/oss-pr-campaign/repos/zerobrew/zb_core/`)
**Purpose:** Core data models and domain logic
**Version:** 0.2.1 | Edition: 2024

Files:
- `src/lib.rs` — module re-exports
- `src/build/` — build plan, build system, install method
- `src/context.rs` — concurrency limits, context, log level, logger handle, paths
- `src/errors.rs` — ConflictedLink, Error enum
- `src/formula/` — bottle selection, formula resolution, types

Key dependencies: `serde`, `serde_json` (workspace)

### zb_io (`/root/oss-pr-campaign/repos/zerobrew/zb_io/`)
**Purpose:** I/O operations — network, storage, extraction, installation
**Version:** 0.2.1 | Edition: 2024

Files:
- `src/lib.rs` — library root
- `src/api.rs` — Homebrew API client
- `src/checksum.rs` — checksum verification
- `src/path.rs` — path utilities
- `src/progress.rs` — progress display
- `src/ssl.rs` — SSL/TLS
- `src/build/` — Ruby source build (shim.rb)
- `src/cellar/` — cellar operations (materialize, link)
- `src/extraction/` — archive extraction (tar, zip, xz, zstd)
- `src/installer/` — installation pipeline (bottle, source, doctor, outdated, plan, uninstall, cask, homebrew)
- `src/network/` — network layer (api, cache, tap_formula, suggest, download/*)
- `src/storage/` — storage layer (blob, db, store)

Key dependencies: `tokio`, `reqwest`, `rusqlite`, `tar`, `zip`, `xz2`, `zstd`, `flate2`, `rayon`, `sha2`, `walkdir`, `fs4`, `rustls`, `regex`, `tracing`, `strsim`, `tempfile`, `wiremock`, `arwen`, `object`

### zb_cli (`/root/oss-pr-campaign/repos/zerobrew/zb_cli/`)
**Purpose:** Command-line interface
**Version:** 0.2.1 | Edition: 2024

Files:
- `src/lib.rs` — library root
- `src/cli.rs` — CLI struct, Commands enum, argument parsing
- `src/commands/` — command implementations
- `src/init.rs` — init command
- `src/logging.rs` — logging setup
- `src/ui.rs` — UI/theme layer
- `src/utils.rs` — utilities
- `src/bin/` — binary targets (`zb`, `zbx`)
- `tests/integration.rs` — integration tests

Key dependencies: `clap`, `clap_complete`, `tokio`, `indicatif`, `console`, `serde_json`, `tracing`, `tracing-subscriber`, `chrono`

## Workspace Configuration

**Root:** `/root/oss-pr-campaign/repos/zerobrew/Cargo.toml`

```toml
[workspace]
members = ["zb_core", "zb_io", "zb_cli"]
resolver = "3"
rust-version = "1.90"
```

**Key workspace dependencies:**
- tokio 1 (full features)
- serde + serde_json
- clap 4 (derive, env)
- reqwest (rustls, stream, json, http2)
- rustls 0.23 (aws-lc-rs)
- flate2, tar, xz2, zstd, zip
- rusqlite (bundled)
- rayon, futures, futures-util
- regex, sha2, walkdir, fs4, libc
- tracing + tracing-subscriber

## CI/CD

**Workflows:** `.github/workflows/`
- `ci.yml` — lint/badge
- `test.yml` — test on macOS (arm64 stable + 1.90) + Ubuntu (stable)
- `homebrew-compat.yml` — Homebrew compatibility
- `release.yml` — release builds

**Test strategy:**
1. `cargo build --workspace --all-targets` (debug)
2. `cargo build --workspace --release`
3. `cargo test --workspace` (unit tests)
4. `cargo test --package zb_cli --test integration -- --ignored` (integration tests)

## Installation / Build

```bash
# Requires Rust 1.90+
cargo build --workspace --release

# Or via just:
just build    # fmt + lint + build
just install  # build + install to $ZEROBREW_BIN
just test     # unit + integration
just fmt      # format
just lint     # clippy
```

## Notable Features

1. **Content-addressable storage** — deduplication via hash-based storage
2. **APFS clonefiles** — zero-overhead copying on macOS
3. **Homebrew bottle compatibility** — uses pre-built bottles when available
4. **Source build fallback** — compiles from Homebrew Ruby DSL when no bottle
5. **Parallel downloads** — chunked, parallel download support
6. **Batch migration** — migrate from Homebrew to zerobrew
7. **`zb doctor`** — state diagnosis and repair
8. **`zb bundle`** — Brewfile support (install/dump)

## Code Stats (approx)

- Rust files: ~100+
- Lines of Rust: ~20K+
- 3 binary targets: `zb` (main CLI), `zbx` (run without linking), `zb` (cli lib)
- Integration test suite via `wiremock`