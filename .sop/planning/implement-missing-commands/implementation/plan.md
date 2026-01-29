# Implementation Plan: Zerobrew Update/Outdated/Upgrade Commands

## Checklist

- [x] Step 1: Add ApiCache management methods
- [x] Step 2: Implement `update` command
- [x] Step 3: Add OutdatedPackage type and detection logic
- [x] Step 4: Implement `outdated` command (basic)
- [x] Step 5: Add `outdated` output formats (quiet, verbose, json)
- [x] Step 6: Implement `upgrade` command (single package)
- [x] Step 7: Implement `upgrade` command (all packages)
- [ ] Step 8: Add dry-run support for upgrade
- [ ] Step 9: Integration tests and edge cases
- [ ] Step 10: Documentation and help text

---

## Step 1: Add ApiCache management methods

**Objective:** Extend `ApiCache` with methods to clear cache and retrieve statistics.

**Implementation guidance:**
- Add `clear()` method that deletes all rows from `api_cache` table
- Add `clear_older_than(max_age_secs)` for selective clearing
- Add `stats()` method returning entry count and age range
- All operations should be single SQL statements for atomicity

**Test requirements:**
- Test `clear()` removes all entries
- Test `clear_older_than()` only removes old entries
- Test `stats()` returns correct counts

**Integration:** These methods will be called by the `update` command.

**Demo:** Run unit tests showing cache can be populated, cleared, and stats retrieved correctly.

---

## Step 2: Implement `update` command

**Objective:** Add the `zb update` CLI command that clears the API cache.

**Implementation guidance:**
- Add `Update` variant to `Commands` enum in `main.rs`
- In command handler:
  1. Open the ApiCache (at `{root}/db/api_cache.sqlite3` or similar)
  2. Call `cache.clear()`
  3. Print result: `"==> Cleared {n} cached formula entries"`
- Handle case where cache doesn't exist or is empty

**Test requirements:**
- Test command runs without error on empty cache
- Test command reports correct count after clearing

**Integration:** Uses `ApiCache` methods from Step 1.

**Demo:** Run `zb update` and see cache cleared message. Run `zb install jq` then `zb update` and verify cache was cleared.

---

## Step 3: Add OutdatedPackage type and detection logic

**Objective:** Add the core logic to detect outdated packages by comparing sha256 hashes.

**Implementation guidance:**
- Add `OutdatedPackage` struct to `zb_io/src/install.rs`:
  ```rust
  pub struct OutdatedPackage {
      pub name: String,
      pub installed_version: String,
      pub installed_sha256: String,
      pub current_version: String,
      pub current_sha256: String,
  }
  ```
- Add `is_outdated(&self, name: &str) -> Result<Option<OutdatedPackage>, Error>` to `Installer`:
  1. Get installed keg from database
  2. Fetch current formula from API
  3. Select bottle for current platform
  4. Compare `installed.store_key` with `bottle.sha256`
  5. Return `Some(OutdatedPackage)` if different, `None` if same

**Test requirements:**
- Test returns `None` when sha256 matches
- Test returns `Some` when sha256 differs
- Test handles missing package (not installed) gracefully

**Integration:** Will be used by `check_outdated()` and `outdated` command.

**Demo:** Unit test demonstrating outdated detection with mock API responses.

---

## Step 4: Implement `outdated` command (basic)

**Objective:** Add `zb outdated` command that lists outdated packages.

**Implementation guidance:**
- Add `Outdated` variant to `Commands` enum (with `quiet`, `verbose`, `json` flags)
- Add `check_outdated(&self) -> Result<Vec<OutdatedPackage>, Error>` to `Installer`:
  1. Get all installed packages from database
  2. Check each in parallel using `futures::future::join_all`
  3. Collect successful results, log warnings for failures
  4. Return list of outdated packages
- In CLI handler, display results in default format:
  ```
  package (installed_version) < current_version
  ```

**Test requirements:**
- Test returns empty list when no packages outdated
- Test returns correct list when packages outdated
- Test continues on individual check failures

**Integration:** Uses `is_outdated()` from Step 3, parallel checking like existing `fetch_all_formulas()`.

**Demo:** Install a package, modify mock API to return different sha256, run `zb outdated` and see package listed.

---

## Step 5: Add `outdated` output formats (quiet, verbose, json)

**Objective:** Support Homebrew-compatible output flags for `outdated`.

**Implementation guidance:**
- `--quiet`: Print only package names, one per line
- `--verbose`: Print detailed info including sha256 (truncated)
  ```
  package (installed_version) < current_version [abc123... -> def456...]
  ```
- `--json`: Output JSON object with `formulae` array
- Add `serde::Serialize` derive to `OutdatedPackage`

**Test requirements:**
- Test quiet output format
- Test verbose output format
- Test JSON output is valid and parseable

**Integration:** Extends Step 4 CLI handler.

**Demo:** Run `zb outdated --quiet`, `zb outdated --verbose`, `zb outdated --json` and verify each format.

---

## Step 6: Implement `upgrade` command (single package)

**Objective:** Add `zb upgrade <formula>` to upgrade a specific package.

**Implementation guidance:**
- Add `Upgrade` variant to `Commands` enum with `formula: Vec<String>` and `dry_run: bool`
- For single package upgrade:
  1. Check if package is installed
  2. Check if package is outdated using `is_outdated()`
  3. If outdated: uninstall old version, install new version
  4. Reuse existing `installer.uninstall()` and `installer.install()`
- Print progress similar to install command

**Test requirements:**
- Test upgrade succeeds for outdated package
- Test upgrade skips up-to-date package with message
- Test upgrade fails gracefully for non-installed package

**Integration:** Uses `is_outdated()` from Step 3, existing install/uninstall from `Installer`.

**Demo:** Install old version of package, run `zb upgrade <package>`, verify new version installed.

---

## Step 7: Implement `upgrade` command (all packages)

**Objective:** Add `zb upgrade` (no args) to upgrade all outdated packages.

**Implementation guidance:**
- When `formula` vec is empty:
  1. Call `check_outdated()` to get all outdated packages
  2. If empty, print "All packages are up to date"
  3. Otherwise, upgrade each package in dependency order
- Handle partial failures: continue upgrading remaining packages, report failures at end
- Exit with non-zero status if any upgrades failed

**Test requirements:**
- Test upgrades multiple packages
- Test continues after individual failure
- Test reports summary at end

**Integration:** Uses `check_outdated()` from Step 4, upgrade logic from Step 6.

**Demo:** Install multiple packages, make some outdated via mock, run `zb upgrade`, verify all upgraded.

---

## Step 8: Add dry-run support for upgrade

**Objective:** Add `--dry-run` / `-n` flag to preview upgrades without executing.

**Implementation guidance:**
- When `dry_run` is true:
  1. Perform all checks (outdated detection)
  2. Print what would be upgraded:
     ```
     Would upgrade:
         package1 (1.0.0) -> (2.0.0)
         package2 (3.0.0) -> (3.1.0)
     ```
  3. Do not call uninstall/install
- Works for both single package and all packages modes

**Test requirements:**
- Test dry-run shows correct packages
- Test dry-run does not modify anything
- Test dry-run exit code is 0

**Integration:** Extends Step 6/7 with conditional execution.

**Demo:** Run `zb upgrade --dry-run` with outdated packages, verify list shown but nothing changed.

---

## Step 9: Integration tests and edge cases

**Objective:** Add comprehensive integration tests covering edge cases.

**Implementation guidance:**
- Add integration tests in `zb_io/src/install.rs` tests module:
  - Network failure during outdated check
  - Package removed from Homebrew (404)
  - Mixed success/failure in batch operations
  - Empty installed list
  - All packages up to date
- Add CLI integration tests if test harness exists

**Test requirements:**
- All edge cases have test coverage
- Tests use mock server (wiremock)
- Tests verify both success and error paths

**Integration:** Uses existing test infrastructure.

**Demo:** Run `cargo test` and see all new tests passing.

---

## Step 10: Documentation and help text

**Objective:** Add help text and update any documentation.

**Implementation guidance:**
- Add doc comments to all new public types and methods
- Ensure CLI help text is clear and matches Homebrew style:
  ```
  zb update      Refresh package metadata cache
  zb outdated    List outdated packages
  zb upgrade     Upgrade outdated packages
  ```
- Update README if maintained in repo

**Test requirements:**
- `zb --help` shows new commands
- `zb update --help`, `zb outdated --help`, `zb upgrade --help` show correct flags

**Integration:** Final polish step.

**Demo:** Run `zb --help` and verify all three new commands appear with descriptions.
