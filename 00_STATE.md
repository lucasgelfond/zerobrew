# zerobrew — Analysis State

**Repository:** `lucasgelfond/zerobrew`
**Fork:** `okwn/zerobrew` (cloned to `/root/oss-pr-campaign/repos/zerobrew`)
**License:** Apache-2.0 OR MIT (dual)
**Language:** Rust
**Stars:** 7,290 | **Forks:** 170 | **Open Issues:** 37

## Status: ✅ Analysis Complete

- Fork created: `okwn/zerobrew`
- Upstream remote: `upstream` → `https://github.com/lucasgelfond/zerobrew`
- All branches fetched (main, cask, feat/zb-cli-cask-flag, fix/audit)
- Repository is NOT archived, allows forking
- Current HEAD: `main` (synced with upstream/main)

## Key Findings

1. **Rust workspace** with 3 crates: `zb_core`, `zb_io`, `zb_cli`
2. **7,290 stars** — high visibility, active project
3. **37 open issues** — good selection of work items
4. **Recent active development** — last commit May 7, 2026
5. **Branch structure:** `main`, `cask`, `feat/zb-cli-cask-flag`, `fix/audit`
6. **No open PRs** — only the fork exists, no upstream PRs
7. **Cargo/rust not installed** in this environment — cannot run tests locally
8. **CI runs on:** macOS (arm64), Ubuntu (stable Rust)

## Repository Metadata

| Field | Value |
|---|---|
| description | A 5-20x faster experimental Homebrew alternative |
| archived | false |
| license | Apache-2.0 |
| language | Rust |
| default branch | main |
| topics | (none set) |
| has issues | true |
| has wiki | true |

## Active Branches

| Branch | Description |
|---|---|
| `main` | Primary development (current HEAD) |
| `cask` | Cask support (experimental) |
| `feat/zb-cli-cask-flag` | CLI cask flag feature |
| `fix/audit` | Audit-related fixes |

## Issues Summary (30 open)

- **bug:** #348 (Ruby NameError), #347 (Pathname issues), #346 (ELF patching), #343 (conflicting symlinks), #342 (store corruption), #340 (entitlement), #245 (CA certs), #188 (link conflicts), #286 (Mach-O patching), #300 (mach-o validation)
- **feature:** #339 (shim layer), #335 (leaves command), #303 (structured tracing), #268 (TUI), #162 (info/list), #150 (migrate --revert)
- **refactor:** #302 (pub visibility audit), #293 (codebase hardening), #271 (unify package struct), #272 (mock test infra)
- **CI:** #315 (ubuntu version)
- **enhancement:** #350 (help messages), #349 (man entry), #341 (api mirroring env vars), #338 (fasd locate), #336 (python linux), #334 (glibc version), #331 (gh update)

## Quality Audit Notes

- Uses `tempfile` + `wiremock` for testing
- CI runs: build (debug), build (release), unit tests, integration tests
- Commits follow format: `fix/feat/chore($crate): description`
- Edition 2024 Rust (cutting edge)
- Workspace resolver version 3