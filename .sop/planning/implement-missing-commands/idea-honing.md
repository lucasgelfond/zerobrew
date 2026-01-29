# Requirements Clarification

This document captures the Q&A process for refining the zerobrew subcommands feature.

---

## Q1: Scope Priority

**Question:** All five commands (`outdated`, `upgrade`, `update`, `cleanup`, `doctor`) are requested. Should we design and plan for all five together, or prioritize a subset for an initial implementation?

**Answer:** Core workflow first - `update` → `outdated` → `upgrade`. This covers the typical user flow and validates the approach before implementing `cleanup` and `doctor`.

---

## Q2: Homebrew Compatibility

**Question:** Should these commands aim for Homebrew CLI compatibility (same flags, similar output format), or is a zerobrew-specific approach acceptable?

**Answer:** Homebrew-compatible - aim for similar flags and output format to ease adoption for existing Homebrew users and ensure scripts work across both tools.

---

## Q3: Package Index Source

**Question:** For the `update` command, where does zerobrew fetch its package index/formulae from? Is there an existing mechanism, or does this need to be designed?

**Answer:** Use Homebrew's formulae API directly (zerobrew already does this). The `update` command would refresh/clear the API cache so subsequent `outdated` checks fetch fresh formula metadata. No local git repo sync needed - aligns with zerobrew's stateless, on-demand design.

---

## Q4: Version Comparison Strategy

**Question:** For `outdated`, how should we compare installed versions against available versions? Options:
- Simple string comparison (may fail on semver edge cases)
- Semantic version parsing (handles 1.2.3 < 1.10.0 correctly)
- Hash-based (compare bottle sha256 - different hash = outdated)

**Answer:** Hash-based comparison. Compare the installed package's store_key (sha256) against the current bottle's sha256 from the API. This catches both version changes and bottle rebuilds, and is simpler than parsing arbitrary version strings.

---

## Q5: Upgrade Behavior

**Question:** When running `upgrade`, should it:
- Upgrade all outdated packages by default (like `brew upgrade`)
- Require explicit package names (like `apt upgrade <pkg>`)
- Support both: `zb upgrade` (all) and `zb upgrade <pkg>` (specific)

**Answer:** Support both: `zb upgrade` upgrades all outdated packages, `zb upgrade <pkg>` upgrades specific package(s). Matches Homebrew behavior.

---

## Q6: Dependency Handling on Upgrade

**Question:** When upgrading a package, should zerobrew also upgrade its dependencies if they're outdated?
- Yes, always upgrade outdated dependencies (Homebrew default)
- No, only upgrade the explicitly requested package
- Add a flag like `--ignore-dependencies` to control this

**Answer:** Yes, always upgrade outdated dependencies. Matches Homebrew default behavior.

---

## Q7: Output Format for `outdated`

**Question:** What information should `zb outdated` display? Homebrew shows:
```
package (installed_version) < new_version
```
Should we match this format, or include additional info (e.g., sha256 diff indicator)?

**Answer:** Match Homebrew's format for compatibility:
- Default (interactive): `package (installed_version) < new_version`
- `--quiet` / `-q`: Just package names (one per line)
- `--verbose` / `-v`: Include detailed version info
- `--json`: Machine-readable JSON output

This matches zerobrew's existing `list` command style (`name version`) and Homebrew's `outdated` flags.

---

## Q8: Error Handling for Unavailable Packages

**Question:** During `outdated` or `upgrade`, if a package's formula can't be fetched from the API (network error, removed formula), should zerobrew:
- Skip silently and continue with other packages
- Warn but continue
- Fail the entire operation

**Answer:** Warn but continue. Print a warning for packages that can't be checked/upgraded, but proceed with the rest. This provides visibility into issues without blocking the entire operation.

---

## Q9: Concurrency for `outdated` Checks

**Question:** Zerobrew uses parallel downloads (default 48 concurrent). Should `outdated` also check packages in parallel against the API?

**Answer:** Yes - parallel API requests for speed. Consistent with zerobrew's performance-focused philosophy. Reuse the existing concurrency infrastructure.

---

## Requirements Complete

Requirements clarification concluded. Ready to proceed to design phase.

