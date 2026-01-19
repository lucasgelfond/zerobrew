# AGENTS.md — Zerobrew v0 Implementation Guide

This repo is built for parallel development by multiple agents. Read this before touching anything.

## 0. Core rules (non-negotiable)

1) **Small files.** One responsibility per file. If a file grows > ~200 lines, split it.
2) **Tests per file.** Every module has unit tests in the same file or a paired `*_test.rs`.
3) **Readable > clever.** Prefer simple structs + functions. Avoid meta-abstraction.
4) **No unnecessary comments.** Code should be self-explanatory. Only comment invariants.
5) **Pure core, I/O at edges.** Core logic should be deterministic and testable.
6) **No global mutable state.** Pass a `Context` object and explicit deps.
7) **Determinism.** Planning order must be stable across runs.

## 1. Working agreements for agents

### 1.1 Branching and PR hygiene
- Make small PRs (one module at a time).
- Include tests in every PR.
- Avoid touching unrelated files.
- Keep public APIs minimal and stable.

### 1.2 Error handling
- Use typed errors. No `anyhow` at module boundaries.
- Errors must be actionable:
  - Unsupported bottle
  - Checksum mismatch
  - Link conflict
  - Store corruption
  - Network failures

### 1.3 Observability
- Use structured events (levels: info/warn/error).
- Avoid chatty logs in tight loops; prefer aggregated progress.

## 2. Module ownership map (parallelizable tasks)

Each bullet is a self-contained module with its own tests. Agents can own these independently.

### A) `zb_io/api.rs` — Homebrew API client
Responsibilities:
- Fetch formula JSON by name
- Respect ETag / Last-Modified cache headers (store in sqlite)
- Retry transient failures (bounded)
Tests:
- Deserialize fixture JSON
- Caching behavior: uses stored ETag, conditional request

### B) `zb_core/resolve.rs` — Dependency closure + planning order
Responsibilities:
- Given a root formula, compute transitive closure
- Deterministic ordering (toposort)
- Cycle detection
Tests:
- Small fixture graphs
- Deterministic output order
- Cycle errors

### C) `zb_core/plan.rs` — Install plan DAG
Responsibilities:
- Convert resolved graph into stages:
  - download set
  - unpack set
  - materialize set
- Identify unsupported nodes early
Tests:
- Plan ordering invariants
- Early failure

### D) `zb_io/cache.rs` — Blob download cache
Responsibilities:
- Atomic downloads: tmp -> rename
- Dedupe: concurrent callers share single download
- Checksum verification function
Tests:
- Simulated parallel downloads (threads)
- Atomicity (no partial final blobs)
- Checksum mismatch handling

### E) `zb_io/store.rs` — Unpacked store
Responsibilities:
- Unpack bottle into immutable store entry keyed by store_key
- Prevent path traversal during extraction
- Idempotent: if exists and valid, skip
Tests:
- Tar fixture extraction correctness:
  - symlinks preserved
  - perms preserved
  - traversal blocked
- Concurrency lock correctness

### F) `zb_io/materialize.rs` — Keg materialization
Responsibilities:
- Create keg dir from store entry using:
  - APFS clonefile, else hardlink, else copy
- Per-file fallback (if clone fails for a file, copy that file)
Tests:
- tempdir tests for:
  - clone/hardlink fallback paths (mock failures)
  - symlinks preserved
  - executable bit preserved

### G) `zb_io/link.rs` — Linking shims into `/opt/homebrew/bin`
Responsibilities:
- Create symlinks for executables
- Maintain conflict policy (v0: error)
- Record linked paths in DB for clean uninstall
Tests:
- Conflict test
- Uninstall cleans links
- Deterministic behavior

### H) `zb_io/db.rs` — SQLite persistence
Responsibilities:
- Schema migrations (v0: single migration)
- Transactions for install/uninstall
- Store refcounts
Tests:
- In-memory sqlite tests
- Transaction rollback safety

### I) `zb_cli` — CLI commands and wiring
Responsibilities:
- Parse args (clap)
- Construct Context
- Call into core/IO layers
Tests:
- Minimal CLI tests (argument parsing)
- Integration tests live elsewhere

### J) `zb_bench` — Benchmark harness
Responsibilities:
- Run `brew` and `zb` scenarios
- Capture timings and deltas
- Emit JSON report
Tests:
- Report schema test
- “mock mode” smoke bench

## 3. Interfaces and invariants (to keep modules composable)

### 3.1 `Context`
A single struct passed to commands:
- paths (root/store/cache/cellar/db)
- http client
- concurrency limits
- logger/progress sink

### 3.2 Store key
A canonical identifier for an unpacked bottle.
- Must be stable and derived from verified content (prefer checksum-based).

### 3.3 Idempotency
All major operations must be safe to repeat:
- download_blob(expected_sha) is idempotent
- ensure_store_entry(store_key) is idempotent
- materialize_keg(name, version, store_key) is idempotent
- link_keg(name, version) is idempotent with defined conflict behavior

### 3.4 No partial states
If an install fails:
- do not leave a half-linked executable in `/opt/homebrew/bin`
- do not leave a half-written store dir
- DB either shows installed state or not; never “maybe”

## 4. Test expectations (agent checklist)

Before submitting a PR, ensure:
- All new functions have unit tests.
- All filesystem tests use `tempfile::TempDir`.
- Network tests use a local mock server (wiremock or tiny test server), never real HTTP.
- Concurrency tests are deterministic (use barriers / controlled scheduling).
- Fixtures are minimal (small JSON and small tar).

## 5. Benchmark expectations (agent checklist)

Any PR that touches performance-sensitive code should:
- not regress `zb bench smoke`
- optionally include a local perf note (before/after numbers) if it changes concurrency or I/O paths

## 6. Style guide

- Prefer explicit types and small enums.
- Avoid deep trait hierarchies.
- Prefer `Result<T, Error>` with a local error enum.
- Keep module public APIs tiny.
- Avoid “manager” classes; use plain functions and small structs.

## 7. Suggested first tasks (good starting tickets)

1) API client + JSON fixtures + tests
2) Resolver + tests
3) Blob cache + download concurrency + tests
4) Store unpack + tar safety + tests
5) Materializer with fallback + tests
6) Minimal end-to-end integration test with mocked API + mocked bottle server
7) Bench harness JSON output + “brew vs zb” runner

