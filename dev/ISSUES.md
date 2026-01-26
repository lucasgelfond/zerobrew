# Zerobrew v0 — Iterative Build Plan (Agent Checklist)

This document is the **authoritative step-by-step plan** for building Zerobrew v0.
Each item is intended to be completed as **one small PR / one or a few commits**.
Agents should **check off items strictly in order**.

Each step has:
- **Action** — what to implement
- **Acceptance** — what must be true to check it off

---

## Phase 0 — Repo Skeleton & Invariants

### ⬜ Step 0.1 — Workspace & CI
**Action**
- Create Rust workspace with crates:
  - `zb_core`
  - `zb_io`
  - `zb_cli`
  - `zb_bench`
- Add `rustfmt` + `clippy`
- Add CI running `cargo test` and `cargo clippy -- -D warnings`

**Acceptance**
- `cargo test` passes
- `cargo clippy -- -D warnings` passes
- Empty tests allowed

---

### ⬜ Step 0.2 — Shared Context & Errors
**Action**
- Define:
  - `Context` struct (paths, concurrency limits, logger handle)
  - `zb_core::errors` (typed error enums)
- Default paths under `/opt/zerobrew/*`, configurable

**Acceptance**
- Unit test: `Context::from_defaults()` sets expected paths
- No `anyhow` in public APIs

---

## Phase 1 — Metadata & Resolution (Pure Logic)

### ⬜ Step 1.1 — Formula Data Model
**Action**
- Define minimal structs for Homebrew formula JSON:
  - name
  - version
  - dependencies
  - bottle metadata (macOS arm64 only)
- Add 2–3 small JSON fixtures

**Acceptance**
- Unit tests deserialize fixtures successfully

---

### ⬜ Step 1.2 — Dependency Resolver
**Action**
- Implement transitive dependency closure
- Deterministic ordering (toposort)
- Cycle detection

**Acceptance**
- Unit tests:
  - correct closure
  - stable order across runs
  - cycle → error

---

### ⬜ Step 1.3 — Bottle Selection
**Action**
- Implement `select_bottle(formula)`
- Choose macOS arm64 bottle
- Error early if missing

**Acceptance**
- Unit test: bottle exists → success
- Unit test: no bottle → `UnsupportedBottle`

---

## Phase 2 — Homebrew API Client (Mocked)

### ⬜ Step 2.1 — API Client v0
**Action**
- Implement `ApiClient::get_formula(name)`
- No real HTTP in tests
- Use `wiremock` or equivalent

**Acceptance**
- Unit test: fetch formula JSON from mock server
- Unit test: 404 → typed error

---

### ⬜ Step 2.2 — API Cache (ETag / If-None-Match)
**Action**
- Add `api_cache` table in sqlite
- Implement conditional GET support

**Acceptance**
- Unit test:
  - First request stores ETag
  - Second request sends `If-None-Match`
  - 304 uses cached body

---

## Phase 3 — Download Cache & Parallelism

### ⬜ Step 3.1 — Blob Cache Layout
**Action**
- Implement:
  - `cache/blobs/`
  - `cache/tmp/`
- Atomic temp → rename writes

**Acceptance**
- Unit test: interrupted write leaves no final blob
- Unit test: completed write produces final blob

---

### ⬜ Step 3.2 — Downloader + Checksum Verification
**Action**
- Stream HTTP download to temp file
- Verify SHA256 against expected checksum
- Delete blob on mismatch

**Acceptance**
- Unit test: valid checksum passes
- Unit test: mismatch deletes blob + errors

---

### ⬜ Step 3.3 — Parallel Downloader (Bounded)
**Action**
- Implement bounded parallel downloads
- Deduplicate concurrent requests by blob hash (single-flight)

**Acceptance**
- Unit test: peak concurrent downloads ≤ limit
- Unit test: same blob requested 5× → 1 HTTP fetch

---

## Phase 4 — Store (Unpack Once)

### ⬜ Step 4.1 — Safe Tar Extraction
**Action**
- Implement tar extraction that:
  - rejects `..` traversal
  - rejects absolute paths
  - preserves symlinks
  - preserves permissions

**Acceptance**
- Unit tests with tiny tar fixture:
  - symlink preserved
  - executable bit preserved
  - traversal rejected

---

### ⬜ Step 4.2 — Store Entry Creation
**Action**
- Implement `ensure_store_entry(store_key, blob_path)`
- Unpack once, idempotent
- Per-store-key lockfile

**Acceptance**
- Unit test: concurrent calls unpack once
- Unit test: second call is no-op

---

## Phase 5 — Materialization (Install Primitive)

### ⬜ Step 5.1 — Materializer v0 (Copy Only)
**Action**
- Implement materialization by copying store → cellar
- Path: `/opt/zerobrew/cellar/<name>/<version>`

**Acceptance**
- Unit test: tree reproduced exactly

---

### ⬜ Step 5.2 — Clonefile / Hardlink / Copy Fallback
**Action**
- Add per-file strategy:
  1) APFS `clonefile`
  2) hardlink
  3) copy

**Acceptance**
- Unit test: forced clone failure falls back
- Unit test: hardlink failure falls back to copy

---

## Phase 6 — Linking & Install State

### ⬜ Step 6.1 — Link Executables
**Action**
- Symlink executables into `/opt/homebrew/bin`
- Deterministic conflict policy: **error**

**Acceptance**
- Unit test: conflict → error
- Unit test: uninstall removes links

---

### ⬜ Step 6.2 — SQLite Install State
**Action**
- Implement:
  - `installed_kegs`
  - `store_refs`
- Use transactions

**Acceptance**
- Unit test: rollback leaves no partial state

---

## Phase 7 — End-to-End Install

### ⬜ Step 7.1 — `zb install` (Mocked)
**Action**
- Wire:
  - resolve
  - plan
  - download
  - unpack
  - materialize
  - link
  - record

**Acceptance**
- Integration test:
  - mocked API + mocked bottles
  - install completes successfully

---

## Phase 8 — Uninstall & GC

### ⬜ Step 8.1 — Uninstall
**Action**
- Remove:
  - linked shims
  - cellar entry
- Decrement store refs

**Acceptance**
- Unit test: uninstall cleans everything

---

### ⬜ Step 8.2 — Garbage Collection
**Action**
- Delete unreferenced store entries
- Optional LRU pruning for blobs

**Acceptance**
- Unit test: referenced store never deleted
- Unit test: unreferenced store removed

---

## Phase 9 — Benchmarking

### ⬜ Step 9.1 — Smoke Bench (Mocked)
**Action**
- `zb bench smoke`
- Resolve + download + install using fixtures

**Acceptance**
- Runtime < 60s
- Deterministic output

---

### ⬜ Step 9.2 — Real Perf Bench (Local)
**Action**
- Implement:
  - Cold install
  - Warm reinstall
- Compare against `brew`

**Acceptance**
- JSON report emitted
- Warm reinstall ≥ 3× faster than brew (target)

---

## Phase 10 — Daily Driver Gate

### ⬜ Step 10.1 — libheif Validation
**Action**
- Run `zb install libheif` on macOS arm64
- Then uninstall and reinstall

**Acceptance**
- Install succeeds
- Warm reinstall measurably faster than brew
- No leftover files

---

## Final Goal
When **all boxes are checked**, Zerobrew v0 is:
- Correct
- Testable
- Measurably faster than Homebrew
- Ready for daily use on macOS arm64
