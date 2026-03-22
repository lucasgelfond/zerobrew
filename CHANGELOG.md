# Changelog

All notable changes to zerobrew will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]
### Added
- Chinese translation of the README ([#315](https://github.com/lucasgelfond/zerobrew/pull/316))
- Regex matches on `/Cellar/<pkg>/)([^/]+)(/)`, so it only matches version segments within Cellar-style paths ([#317](https://github.com/lucasgelfond/zerobrew/pull/317))

### Added
- `zb doctor` command with `--repair` flag for state diagnosis and recovery ([#314](https://github.com/lucasgelfond/zerobrew/pull/314))

### Fixed
- Validate root/prefix paths before passing to sudo to prevent shell injection ([#311](https://github.com/lucasgelfond/zerobrew/pull/311))

### Changed
- Split monolithic install module into focused submodules ([#312](https://github.com/lucasgelfond/zerobrew/pull/312))
- Split monolithic download module into focused submodules ([#313](https://github.com/lucasgelfond/zerobrew/pull/313))


## [0.2.1] - 2026-03-14

### Fixed
- Fix `zb outdated` panic caused by clap type mismatch between global `verbose` (u8 count) and subcommand `verbose` (bool) flags ([#308](https://github.com/lucasgelfond/zerobrew/pull/308))

## [0.2.0] - 2026-03-12

### Added
- Batch processing for `zb migrate` command ([#285](https://github.com/lucasgelfond/zerobrew/pull/285))
- `zb outdated` command with `--quiet`/`--verbose`/`--json` output modes ([#266](https://github.com/lucasgelfond/zerobrew/pull/266))
- `zb update` command ([#266](https://github.com/lucasgelfond/zerobrew/pull/266))
- Tracing-based internal logging with `-v`/`--verbose` and `-q`/`--quiet` flags ([#275](https://github.com/lucasgelfond/zerobrew/pull/275))
- Configurable UI theme and writer-based output layer ([#274](https://github.com/lucasgelfond/zerobrew/pull/274))
- Fuzzy formula suggestions on missing package errors ([#279](https://github.com/lucasgelfond/zerobrew/pull/279))
- `ZEROBREW_API_URL` support and persistent API cache ([#252](https://github.com/lucasgelfond/zerobrew/pull/252))
- Build provenance attestation in release workflow ([#247](https://github.com/lucasgelfond/zerobrew/pull/247))

### Fixed
- Added SQLite schema versioning with sequential migrations and downgrade protection([#305](https://github.com/lucasgelfond/zerobrew/pull/305))
- Global lock on installer to prevent concurrent install corruption ([#304](https://github.com/lucasgelfond/zerobrew/pull/304))
- Strip zerobrew's bin paths from `PATH` during install to prevent dyld errors on re-install ([#289](https://github.com/lucasgelfond/zerobrew/pull/289))
- Warn when Mach-O in-place patching is skipped due to prefix length mismatch (Intel Mac) ([#286](https://github.com/lucasgelfond/zerobrew/issues/286))
- Prefer compatible macOS bottle tags over newer ones ([#283](https://github.com/lucasgelfond/zerobrew/pull/283))
- Ruby syntax backwards compatibility for source builds ([#282](https://github.com/lucasgelfond/zerobrew/pull/282))
- Skip extraction on raw binaries and copy to keg bin dir directly ([#278](https://github.com/lucasgelfond/zerobrew/pull/278))
- Chunked download robustness and memory efficiency ([#270](https://github.com/lucasgelfond/zerobrew/pull/270))
- Skip libexec virtualenv metadata links to avoid cross-formula conflicts ([#248](https://github.com/lucasgelfond/zerobrew/pull/248))
- Link formulas on Linux when Homebrew marks them keg-only ([#249](https://github.com/lucasgelfond/zerobrew/pull/249))
- Preprocess resolver before parsing ([#244](https://github.com/lucasgelfond/zerobrew/pull/244))

### Changed
- Eliminate unwraps, reduce allocations, decompose install path ([#292](https://github.com/lucasgelfond/zerobrew/pull/292))
- Removed `--yes` alias from global `--auto-init` flag ([#287](https://github.com/lucasgelfond/zerobrew/pull/287))

## [0.1.2] - 2026-02-15

### Added
- Local source build fallback — compile packages from source when no bottle is available ([#212](https://github.com/lucasgelfond/zerobrew/pull/212))
- `--build-from-source` / `-s` flag for `zb install` ([#212](https://github.com/lucasgelfond/zerobrew/pull/212))
- External tap and cask support with safer install/uninstall behavior ([#203](https://github.com/lucasgelfond/zerobrew/pull/203))
- GitHub release installs with clone fallback ([#198](https://github.com/lucasgelfond/zerobrew/pull/198))
- Source-only tap formula support with scoped parsing ([#232](https://github.com/lucasgelfond/zerobrew/pull/232))
- Resolve tap formulas from `Formula/`, `HomebrewFormula/`, and repo root ([#231](https://github.com/lucasgelfond/zerobrew/pull/231))
- `zb bundle dump` subcommand with Brewfile syntax support ([#218](https://github.com/lucasgelfond/zerobrew/pull/218))

### Fixed
- Include zbx binaries in GitHub releases ([#229](https://github.com/lucasgelfond/zerobrew/pull/229))
- Preserve execute bit when patching Mach-O binary strings ([#228](https://github.com/lucasgelfond/zerobrew/pull/228))
- Skip patching when new prefix is longer than old ([#227](https://github.com/lucasgelfond/zerobrew/pull/227))
- Prevent bricked installs from link conflicts, respect keg-only formulas ([#207](https://github.com/lucasgelfond/zerobrew/pull/207))
- Default macOS prefix to `/opt/zerobrew` to stay within the 13-char Mach-O path limit ([#206](https://github.com/lucasgelfond/zerobrew/pull/206))
- Shell init management and fish support ([#200](https://github.com/lucasgelfond/zerobrew/pull/200))
- Remove `-D` flag from install since directories are already created ([#221](https://github.com/lucasgelfond/zerobrew/pull/221))
- Force static liblzma linking and verify macOS binaries ([#222](https://github.com/lucasgelfond/zerobrew/pull/222))
- Formula token normalization across crates ([#230](https://github.com/lucasgelfond/zerobrew/pull/230))
- Default macOS prefix to root on install scripts ([#239](https://github.com/lucasgelfond/zerobrew/pull/239))

### Changed
- Refreshed README with banner and star history ([#224](https://github.com/lucasgelfond/zerobrew/pull/224))

## [0.1.1] - 2026-02-08

Initial release of zerobrew - a fast, modern package manager. We're excited for our pilot release and 
want to thank all of the support from all channels, as well as all of our contributors up to this point. 

To get an idea of the initial features zerobrew supports, take a look at the [README](https://github.com/lucasgelfond/zerobrew#readme).

See the [full commit history](https://github.com/lucasgelfond/zerobrew/commits/v0.1.1) for more details.

[Unreleased]: https://github.com/lucasgelfond/zerobrew/compare/v0.2.1...HEAD
[0.2.1]: https://github.com/lucasgelfond/zerobrew/compare/v0.2.0...v0.2.1
[0.2.0]: https://github.com/lucasgelfond/zerobrew/compare/v0.1.2...v0.2.0
[0.1.2]: https://github.com/lucasgelfond/zerobrew/compare/v0.1.1...v0.1.2
[0.1.1]: https://github.com/lucasgelfond/zerobrew/releases/tag/v0.1.1
