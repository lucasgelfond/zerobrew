# Changelog

All notable changes to zerobrew will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- Local source build fallback — compile packages from source when no bottle is available ([#212](https://github.com/lucasgelfond/zerobrew/pull/212))
- `--build-from-source` / `-s` flag for `zb install` ([#212](https://github.com/lucasgelfond/zerobrew/pull/212))
- External tap and cask support with safer install/uninstall behavior ([#203](https://github.com/lucasgelfond/zerobrew/pull/203))
- GitHub release installs with clone fallback ([#198](https://github.com/lucasgelfond/zerobrew/pull/198))

### Fixed
- Prevent bricked installs from link conflicts, respect keg-only formulas ([#207](https://github.com/lucasgelfond/zerobrew/pull/207))
- Default macOS prefix to `/opt/zerobrew` to stay within the 13-char Mach-O path limit ([#206](https://github.com/lucasgelfond/zerobrew/pull/206))
- Shell init management and fish support ([#200](https://github.com/lucasgelfond/zerobrew/pull/200))
- Remove `-D` flag from install since directories are already created ([#221](https://github.com/lucasgelfond/zerobrew/pull/221))
- Force static liblzma linking and verify macOS binaries ([#222](https://github.com/lucasgelfond/zerobrew/pull/222))
- Skip patching when new prefix is longer than old ([#227](https://github.com/lucasgelfond/zerobrew/pull/227))

### Changed
- Refreshed README with banner and star history ([#224](https://github.com/lucasgelfond/zerobrew/pull/224))

## [0.1.1] - 2026-02-08

Initial release of zerobrew - a fast, modern package manager. We're excited for our pilot release and 
want to thank all of the support from all channels, as well as all of our contributors up to this point. 

To get an idea of the initial features zerobrew supports, take a look at the [README](https://github.com/lucasgelfond/zerobrew#readme).

See the [full commit history](https://github.com/lucasgelfond/zerobrew/commits/v0.1.1) for more details.

[Unreleased]: https://github.com/lucasgelfond/zerobrew/compare/v0.1.1...HEAD
[0.1.1]: https://github.com/lucasgelfond/zerobrew/releases/tag/v0.1.1
