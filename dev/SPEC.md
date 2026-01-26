# SPECmd — Zerobrew v0 (macOS arm64, bottles-only)

## 0. Goals

Build a Homebrew-compatible *bottles-only* installer for **macOS arm64** that feels “uv-fast” by:
- downloading bottles in parallel,
- unpacking each bottle **once** into a global store,
- “installing” by **APFS clonefile → hardlink → copy** fallback,
- keeping a strong, testable spec and measurable performance wins vs `brew`.

## 1. Non-Goals (explicitly out of scope)

- Linux support
- x86_64 support
- Casks
- Source builds / formula `install` Ruby execution
- Services (launchd/systemd)
- Taps beyond Homebrew core API (v0 uses the Homebrew formula API only)
- Perfect compatibility with every edge case / brew bug-for-bug behavior
- “Adopting” existing `/opt/homebrew/Cellar` kegs (optional later)

## 2. Supported Platform and Constraints

- OS: macOS
- Arch: arm64 only
- FS: assume APFS available; must still function if clonefile fails (fallbacks required)
- Prefix: standard Homebrew prefix:
  - `/opt/homebrew` is the target prefix for linking shims (bin)
  - Zerobrew **does not** write into Homebrew’s Cellar
- Bottles-only:
  - If no bottle exists for macOS arm64, error early with a clear message.

## 3. Compatibility Surface

### 3.1 Registry source (Homebrew API)
Use Homebrew’s JSON API for formula metadata:
- `https://formulae.brew.sh/api/formula/<name>.json`
- Dependencies and bottle URLs/checksums are read from API payloads.

### 3.2 What “install” means in v0
Given `zb install <formula>`:
1) Resolve dependency closure from API metadata (transitive deps).
2) Select the correct bottle for each node (macOS arm64).
3) Download all required bottle tarballs in parallel (bounded concurrency).
4) Verify each tarball’s checksum.
5) Unpack each tarball once into the global store (content-addressed).
6) Materialize a keg directory (a Cellar-like tree under Zerobrew) from the store using clone/hardlink/copy.
7) Link executables into `/opt/homebrew/bin` (symlinks) with deterministic conflict behavior.
8) Record install state in a local SQLite DB.
9) Command returns success only when final state is consistent.

### 3.3 Deterministic conflict policy (v0)
When two kegs provide the same executable name:
- v0 policy: **error** and refuse to link the second conflicting executable.
- Must remain deterministic and test-covered.

## 4. On-disk layout

All Zerobrew state lives under:

- Root: `/opt/zerobrew`
  - `store/` — unpacked, content-addressed bottle payloads
    - `store/<store_key>/...` (immutable)
  - `cellar/` — materialized kegs
    - `cellar/<name>/<version>/...`
  - `cache/` — downloaded bottle tarballs (blobs)
    - `cache/blobs/<sha256>.tar.gz`
    - `cache/tmp/` for in-progress downloads
  - `db/zb.sqlite3`
  - `locks/` — lockfiles for cross-process safety

Notes:
- Never write into Homebrew’s Cellar.
- Linking only writes symlinks into `/opt/homebrew/bin` (and maybe `/opt/homebrew/sbin` later, but not in v0).

## 5. Data model

### 5.1 Formula identity
- `FormulaName` (string, normalized)
- `Version` (string, from API)
- `BottleKey` (derived from bottle URL + checksum, or directly from checksum)

### 5.2 SQLite schema (v0 minimal)
Tables:

- `installed_kegs`
  - `name TEXT`
  - `version TEXT`
  - `store_key TEXT` (immutable store entry)
  - `installed_at INTEGER`
  - PRIMARY KEY (`name`)

- `keg_files` (optional but useful for unlink/uninstall correctness)
  - `name TEXT`
  - `version TEXT`
  - `linked_path TEXT` (e.g., /opt/homebrew/bin/foo)
  - `target_path TEXT` (e.g., /opt/zerobrew/cellar/name/ver/bin/foo)

- `store_refs`
  - `store_key TEXT`
  - `refcount INTEGER`

- `api_cache`
  - `url TEXT PRIMARY KEY`
  - `etag TEXT NULL`
  - `last_modified TEXT NULL`
  - `cached_at INTEGER`

## 6. Concurrency and pipeline

### 6.1 Two-stage bounded pipeline
- Stage A: download (network-bound)
- Stage B: verify + unpack (disk/CPU-bound)
- Stage C: materialize + link (disk-bound)

Concurrency caps (defaults; configurable):
- `download_concurrency = 16`
- `unpack_concurrency = 4`
- `materialize_concurrency = 4`

### 6.2 Locks (must prevent corruption)
- Per-blob download lock keyed by expected sha256.
- Per-store-entry lock keyed by `store_key` (unpack is single-writer).
- Per-formula install lock keyed by formula name (avoid concurrent installs of same thing).
- Global DB write lock (SQLite already serializes; still avoid long transactions).

## 7. Security and correctness requirements

- Safe tar extraction: must prevent path traversal (`..`, absolute paths).
- Preserve symlinks exactly (do not dereference).
- Preserve executable bits.
- Checksum verification is mandatory before unpacking.
- Atomic file operations:
  - download to temp → fsync optional → rename to final blob path
  - unpack to temp dir → rename to final store_key dir
- Failures must leave system in a consistent state:
  - no half-written store dirs
  - no broken linked shims
  - DB reflects reality

## 8. CLI commands (v0)

- `zb install <formula> [--no-link]`
- `zb uninstall <formula>`
- `zb list`
- `zb info <formula>`
- `zb doctor`
- `zb gc`
- `zb cache prune [--max-bytes N] [--max-age-days N]`
- `zb bench smoke` (mocked)
- `zb bench perf --suite <name>` (real)

## 9. Testing strategy

### 9.1 Unit tests (required per module)
- Every file/module has its own unit tests.
- Tests must not require network access unless explicitly marked integration.
- Use `tempfile` for filesystem tests.
- Keep fixtures minimal.

### 9.2 Integration tests (few but real)
- Happy path install with mocked API + mocked bottle server:
  - resolve → download → verify → unpack → materialize → link → record
- Missing bottle returns `UnsupportedBottle` early.
- Checksum mismatch deletes blob and retries once; then fails if still mismatched.
- Concurrent installs of same formula do not corrupt store/cache (two processes or threads).

### 9.3 Spec checkpoints (must pass)
A. Resolver correctness (closure + deterministic order)
B. Bottle selection correctness (macOS arm64)
C. Download atomicity + dedupe
D. Unpack idempotency
E. Materialization fallback logic
F. Link conflict policy enforcement
G. GC correctness (never deletes referenced store)

## 10. Benchmarking and performance gates

### 10.1 Benchmark suites
- `smoke` (mocked):
  - uses local fixtures
  - runtime < 60s
  - must run in CI on every commit
- `perf` (real):
  - compares against `brew` on the same machine
  - runs locally or nightly (not required on every PR)

### 10.2 Perf scenarios (real)
S1 Cold install:
- clear `zb` cache/store and brew caches (best effort)
- time install of suite
S2 Warm reinstall:
- uninstall suite
- reinstall immediately
S3 Bulk install:
- suite of 10–30 formulas with nontrivial deps

### 10.3 Metrics captured
- wall-clock time
- bytes downloaded
- store delta size
- cellar delta size
- number of linked shims
- concurrency achieved (peak in-flight downloads)

### 10.4 Pass/fail gates (initial targets)
- S2 Warm reinstall: `zb` must be **>= 3x faster** than brew median
- S1 Cold install: `zb` must be **not slower than brew by >10%**
- Disk: `zb (store+cellar delta)` should be <= `brew cellar delta` on warm reinstall

## 11. Definition of Done (v0)

- `zb install libheif` succeeds on macOS arm64 with standard prefix on a clean machine.
- `zb uninstall libheif` removes linked shims and keg from zb cellar.
- Warm reinstall of `libheif` is measurably faster than brew (bench harness proves it).
- `zb gc` is safe and does not delete referenced store entries.
- All tests pass: unit + integration + smoke bench.

