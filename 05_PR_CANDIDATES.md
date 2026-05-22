# zerobrew — PR Candidates

Ranked list of issues that represent good PR opportunities.

---

## Tier 1: Good First Issues (entry-level, clear scope)

### #349 — No man entry for zb
- **Labels:** none
- **Summary:** `man zb` returns "No manual entry for zb". Need to generate man pages.
- **Why good:** Single deliverable, well-defined, low risk
- **Effort:** Low
- **Files likely touched:** `install-completions.sh`, `Justfile`, docs
- **Approach:** Use `clap_complete` manpage generation or hand-write roff

### #350 — Add help messages for CLI commands
- **Labels:** none
- **Summary:** CLI commands lack helpful descriptions/help text beyond bare subcommand names
- **Why good:** User-facing polish, clear requirements
- **Effort:** Low-Medium
- **Files likely touched:** `zb_cli/src/cli.rs` (add `#[derive(Parser)]` help attrs)

---

## Tier 2: Medium Effort, High Impact

### #297 — Enable integration tests in CI
- **Labels:** CI, tests
- **Summary:** Integration tests in `zb_cli/tests/integration.rs` are all `#[ignore]`. Need at least one non-ignored smoke test + CI job.
- **Why good:** Directly improves test coverage, follow-up to #293 (hardening)
- **Effort:** Medium
- **Files likely touched:** `zb_cli/tests/integration.rs`, `.github/workflows/test.yml`
- **Approach:** Un-ignore at least one test, verify CI runs them

### #303 — Add structured tracing throughout install pipeline
- **Labels:** feature, zb_cli, zb_io
- **Summary:** Sporadic `tracing::warn` only. Need instrumented tracing for downloads, extraction timing, linking, DB transactions, retries.
- **Why good:** Part of #293, improves observability for a critical path
- **Effort:** Medium
- **Files likely touched:** `zb_io/src/network/`, `zb_io/src/installer/`, `zb_io/src/storage/`, `zb_io/src/extraction/`
- **Approach:** Add `#[instrument]` attrs or explicit `tracing::info!/debug!` spans

### #302 — Audit pub visibility across zb_io
- **Labels:** refactor, zb_cli, zb_core, zb_io
- **Summary:** Most types/functions are `pub` via `pub mod`; internal helpers exposed. Need `pub(crate)` and clear API boundary in `lib.rs`.
- **Why good:** Code quality/immutability improvement, well-scoped refactor
- **Effort:** Medium
- **Files likely touched:** `zb_io/src/` (all modules), `zb_io/src/lib.rs`
- **Approach:** Audit each module, mark internal helpers `pub(crate)`, re-export public API

### #341 — Add homebrew api mirroring environment variables support
- **Labels:** none
- **Summary:** Need to support `HOMEBREW_API_DOMAIN` and `HOMEBREW_BOTTLE_DOMAIN` environment variables for users behind mirrors
- **Why good:** Improves accessibility for Chinese/international users
- **Effort:** Low-Medium
- **Files likely touched:** `zb_io/src/api.rs`, `zb_io/src/network/`
- **Approach:** Read env vars, use in API URL construction

---

## Tier 3: Higher Effort, Significant Value

### #335 — Leaves command (show manually-installed packages)
- **Labels:** none
- **Summary:** Feature request to expose which packages were installed intentionally vs as dependencies (like `brew leaves`)
- **Why good:** Frequently requested feature for Homebrew users
- **Effort:** Medium-High
- **Files likely touched:** `zb_cli/src/commands/`, `zb_io/src/cellar/`, `zb_io/src/storage/`
- **Approach:** Track install reason in DB, filter for top-level deps

### #315 — Change release runner from ubuntu:latest to ubuntu:22.04
- **Labels:** CI
- **Summary:** Ubuntu latest is moving; need to pin to LTS for stability
- **Why good:** Simple change, reduces future CI breakage risk
- **Effort:** Low
- **Files likely touched:** `.github/workflows/release.yml`

### #293 — Codebase hardening: concurrency, state, testing, structure
- **Labels:** high priority, refactor
- **Summary:** Meta-issue tracking multiple hardening sub-tasks (#286, #297, #302, #303)
- **Why good:** High-level umbrella; contributes to multiple sub-projects
- **Effort:** Variable
- **Approach:** Work on any of the sub-issues

### #334 — Lower glibc version
- **Labels:** none
- **Summary:** Binary requires glibc 2.39+, can't run on older systems (Google Colab)
- **Why good:** Increases compatibility surface
- **Effort:** Medium (would need rebuild configuration changes)
- **Files likely touched:** CI configs, possibly `rust-toolchain` file

---

## Tier 4: Challenging / Large Scope

### #188 — Link Conflicts Break Keg Installation (and other linking issues)
- **Labels:** bug, high priority, zb_cli, zb_core, zb_io, refactor
- **Summary:** Multiple linking issues: conflicts can brick installs, pollute environment, symlink overwrites
- **Why good:** Critical bug fix, high visibility
- **Effort:** High
- **Approach:** Complex; requires careful analysis of `zb_io/src/cellar/` linking logic

### #286 — Mach-O patching skipped when zerobrew prefix longer than Homebrew prefix
- **Labels:** bug, high priority, zb_io
- **Summary:** On Intel Mac, binaries get wrong paths patched because prefix is longer than homebrew's
- **Why good:** Important macOS bug
- **Effort:** High
- **Files likely touched:** `zb_io/src/extraction/patch/macos.rs`

### #342 — Store corruption when zerobrew prefix longer than /opt/homebrew
- **Labels:** (unlabeled)
- **Summary:** Path length validation bug causing store corruption
- **Why good:** Critical data integrity issue
- **Effort:** Medium-High
- **Files likely touched:** `zb_io/src/storage/`

### #339 — Add a shim layer
- **Labels:** feature
- **Summary:** Scripts with hardcoded shebangs pointing to cellar paths break. Need shim layer.
- **Why good:** Major UX improvement
- **Effort:** High
- **Approach:** Design and implement shim wrapper

### #271 — Unify package types into a single Package struct
- **Labels:** zb_io, refactor, low priority
- **Summary:** Multiple package type structs should be unified
- **Why good:** Clean code refactor
- **Effort:** Medium
- **Files likely touched:** `zb_core/src/formula/`, `zb_io/src/`

---

## Label Filter

| Label | Issues |
|---|---|
| good first issue | (none labeled — use #349, #350, #341 as equivalents) |
| bug | #188, #286, #300, #342, #346, #347, #348, #343, #245 |
| feature | #339, #335, #303, #268, #162, #150, #324 |
| refactor | #302, #293, #271, #272 |
| CI | #315, #297 |
| high priority | #188, #286, #293 |
| zb_cli | #188, #245, #303, #302, #162, #150 |
| zb_io | #188, #286, #303, #302, #300, #271, #272 |
| zb_core | #188, #302 |