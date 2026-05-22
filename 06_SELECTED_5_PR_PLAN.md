# zerobrew — Selected 5 PR Plan

**Chosen PRs** (balanced: effort, impact, skill match, clear scope):

---

## PR 1: `#349` — No man entry for zb (LOW EFFORT, HIGH VISIBILITY)

### Summary
Generate and install man pages for `zb` command so `man zb` works.

### Motivation
- Simple, well-scoped issue
- Highly visible (affects every user running `man zb`)
- Low risk: adds files, doesn't modify core logic
- Good first contribution

### Files to Change
1. Create `man/zb.1` — roff man page for `zb` (or generate via `clap_complete::man`)
2. Modify `install-completions.sh` — also install man page
3. Or add `just man` recipe to generate man pages

### Implementation Plan
1. Add `clap_complete` man page generation in `zb_cli/src/cli.rs` or a build script
2. Create `man/` directory with `zb.1` roff template
3. Update `install-completions.sh` to install `man/zb.1` to system man path
4. Update `CONTRIBUTING.md` if needed
5. Test: `man zb` shows help after install

### Verification
- [ ] `man zb` displays help text
- [ ] All subcommands (install, uninstall, bundle, etc.) documented
- [ ] CI passes

---

## PR 2: `#350` — Add help messages for CLI commands (LOW EFFORT, POLISH)

### Summary
Add `about = "..."` descriptions and `help = "..."` args to CLI commands so `zb install --help` gives useful context.

### Motivation
- User experience improvement
- Part of the contributor's "UX is important" comment
- Straightforward clap annotations
- Affects all users

### Files to Change
- `zb_cli/src/cli.rs` — Add doc comments and help strings to all Commands variants and their args

### Implementation Plan
1. Go through `Commands` enum variants
2. Add `about = "..."` to each subcommand variant
3. Add `help = "..."` to each arg field
4. Example: `Install { #[arg(long, help = "...")] no_link: bool }`
5. Use the issue author's feedback: "I can only guess what a command does by its name"

### Verification
- [ ] `zb --help` shows descriptions for each command
- [ ] `zb install --help` shows help for `--no-link`, `--build-from-source`
- [ ] `zb bundle --help` shows subcommands
- [ ] CI passes

---

## PR 3: `#297` — Enable integration tests in CI (MEDIUM EFFORT, TEST QUALITY)

### Summary
Enable at least one non-ignored integration test in `zb_cli/tests/integration.rs` and set up CI to run them.

### Motivation
- Follow-up to #293 (hardening)
- Current state: all integration tests are `#[ignore]` — they exist but never run
- CI already has the job defined, just needs un-ignored tests

### Files to Change
1. `zb_cli/tests/integration.rs` — Un-ignore at least one smoke test
2. `.github/workflows/test.yml` — (already configured, just verify it runs with un-ignored tests)

### Implementation Plan
1. Read `zb_cli/tests/integration.rs` — understand existing tests
2. Choose one test that doesn't require real network/homebrew (wiremock-based)
3. Remove `#[ignore]` from that test
4. Add a basic smoke test if none exist: test `zb --version` or `zb help`
5. Ensure CI matrix includes this (already does: macos + ubuntu, stable + 1.90)

### Verification
- [ ] `cargo test --package zb_cli --test integration -- --ignored` runs
- [ ] CI shows integration test job passing
- [ ] At least one integration test runs in CI

---

## PR 4: `#341` — Add homebrew API mirroring env vars support (MEDIUM EFFORT, INTERNATIONAL)

### Summary
Support `HOMEBREW_API_DOMAIN` and `HOMEBREW_BOTTLE_DOMAIN` environment variables for users behind mirrors.

### Motivation
- International users (especially Chinese homebrew mirror users) need this
- Simple: read env vars, use in URL construction
- Matches existing pattern for `ZEROBREW_API_URL`

### Files to Change
1. `zb_io/src/network/api.rs` — read and apply `HOMEBREW_API_DOMAIN`, `HOMEBREW_BOTTLE_DOMAIN`
2. `zb_io/src/lib.rs` or relevant module — expose URL configuration
3. Possibly `zb_io/src/installer/install/mod.rs` for bottle domain

### Implementation Plan
1. Check how `ZEROBREW_API_URL` is currently handled
2. Add similar handling for `HOMEBREW_API_DOMAIN` → sets API base URL
3. Add `HOMEBREW_BOTTLE_DOMAIN` → sets bottle download base URL
4. Fall back to Homebrew defaults when env vars not set
5. Update tests to cover mirror configurations

### Verification
- [ ] `HOMEBREW_API_DOMAIN=https://mirrors.example.com zb install curl` uses mirror
- [ ] `HOMEBREW_BOTTLE_DOMAIN=https://mirrors.example.com zb install curl` uses mirror
- [ ] Without env vars, behavior unchanged (default Homebrew URLs)
- [ ] CI passes

---

## PR 5: `#315` — Pin release runner from ubuntu:latest to ubuntu:22.04 (LOW EFFORT, CI STABILITY)

### Summary
Change `.github/workflows/release.yml` from `ubuntu-latest` to `ubuntu-22.04` to prevent CI breakage when Ubuntu moves to a new version.

### Motivation
- Critical CI stability issue
- "ubuntu-latest" is already Ubuntu 24.04 in some contexts, which is EOL sooner
- Prevents future breakage
- Very simple change

### Files to Change
- `.github/workflows/release.yml` — Replace `runs-on: ubuntu-latest` with `runs-on: ubuntu-22.04`

### Implementation Plan
1. Read `.github/workflows/release.yml`
2. Find all `ubuntu-latest` references (likely in jobs that download packages)
3. Replace with `ubuntu-22.04`
4. Verify no other ubuntu references remain
5. Test: ensure workflow still works (may need to adjust if pkg paths differ)

### Verification
- [ ] All `runs-on` in release.yml use `ubuntu-22.04`
- [ ] CI passes on this change
- [ ] Release workflow still functions

---

## Summary Table

| # | Issue | Effort | Files | Type |
|---|---|---|---|---|
| 1 | #349 No man entry | Low | `man/`, `install-completions.sh` | New files + script |
| 2 | #350 Help messages | Low | `zb_cli/src/cli.rs` | Enhancement |
| 3 | #297 Enable integration tests | Medium | `zb_cli/tests/integration.rs`, CI | Test quality |
| 4 | #341 API mirroring env vars | Medium | `zb_io/src/network/api.rs` | Feature |
| 5 | #315 Pin ubuntu to 22.04 | Low | `.github/workflows/release.yml` | CI stability |

**Total: 2 Low, 2 Medium, 1 Low** — good mix of quick wins and meaningful work.

---

## Execution Notes

- PRs 1, 2, 5 are independently executable (no interdependencies)
- PRs 3 and 4 can be done in parallel after cloning
- All PRs can be based on `upstream/main`
- Each PR should follow commit format: `fix/feat($crate): description`
- Use `just fmt` and `just lint` before opening PR
- Cargo/rust not available in this environment; testing would need to happen in a proper Rust environment or CI