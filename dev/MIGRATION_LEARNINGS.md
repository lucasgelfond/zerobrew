# Migration Feature: Implementation Learnings

> **Status**: Implemented in v0.1.0
> **Created**: January 2026
> **Repository**: https://github.com/chrischabot/zerobrew

This document captures the learnings from implementing the Homebrew to Zerobrew migration feature, intended as a reference for future development sessions.

## What Was Implemented

### New `zb_migrate` Crate

A new crate was added to handle all migration-related functionality:

```
zb_migrate/
├── Cargo.toml
└── src/
    ├── lib.rs          # Module exports
    ├── tab.rs          # INSTALL_RECEIPT.json parser
    ├── scanner.rs      # Homebrew Cellar scanner
    └── migrate.rs      # Migration planning and execution
```

### CLI Command

```bash
zb migrate --dry-run                           # Preview migration
zb migrate                                     # Migrate all user-requested packages
zb migrate jq wget git                         # Migrate specific packages
zb migrate --homebrew-prefix=/custom/path      # Use custom Homebrew location
```

## Key Architectural Decisions

### 1. Fresh Install vs Bottle Adoption

**Decision**: Fresh install approach (re-download bottles from Homebrew CDN)

**Rationale**:
- Simpler implementation - reuses existing `Installer` infrastructure
- Cleaner state - no risk of importing corrupted or mismatched bottles
- Consistent - all packages go through the same install path
- Avoids complexity of verifying existing bottle integrity

**Alternative considered**: "Bottle adoption" where existing Homebrew bottles could be imported directly into Zerobrew's content-addressable store. This was deferred as a potential future optimization.

### 2. Tab File Parsing

**Learned**: Homebrew's `INSTALL_RECEIPT.json` (Tab) format is straightforward JSON but has some quirks:
- `runtime_dependencies` can be `null` or an array
- `source.tap` can be `null`, empty string, or a tap name
- `installed_on_request` vs `installed_as_dependency` together determine if user explicitly requested the package

**Implementation**: Used serde with `#[serde(default)]` liberally to handle missing/null fields gracefully.

### 3. Filtering Strategy

**Decision**: Only migrate "user-requested" packages, letting Zerobrew resolve dependencies fresh

**Rationale**:
- Dependencies may have changed between Homebrew's install time and now
- Zerobrew's dependency resolution will pull in correct current versions
- Avoids importing stale or orphaned dependencies

**Filter logic**:
```rust
// Migrate if user requested OR if it's not marked as a dependency
f.installed_on_request || !f.installed_as_dependency
```

### 4. Custom Tap Handling

**Decision**: Mark packages from non-core taps as "incompatible"

**Rationale**:
- Zerobrew is API-only and doesn't support taps (per SPEC.md)
- Packages from custom taps can't be resolved through the Homebrew formula API
- Better to clearly communicate incompatibility than fail silently

### 5. Services Handling

**Decision**: Detect and warn about running services, but do not migrate them

**Rationale**:
- Services are explicitly out of scope for v0 (per SPEC.md)
- Attempting to manage services during migration could cause system instability
- User should manually manage services before/after migration

## Testing Approach

### Unit Tests (6 total in zb_migrate)

1. `tab::tests::parse_minimal_tab` - Basic Tab parsing
2. `tab::tests::parse_tab_with_dependencies` - Tab with runtime deps
3. `tab::tests::parse_tab_with_custom_tap` - Non-core tap detection
4. `scanner::tests::scan_finds_formulas` - Cellar scanning
5. `scanner::tests::scan_requested_filters_dependencies` - Filter logic
6. `scanner::tests::missing_cellar_returns_error` - Error handling

### Integration Testing

Used mock Homebrew Cellar structure for testing:
```bash
mkdir -p /tmp/test-homebrew/Cellar/jq/1.7.1
cat > /tmp/test-homebrew/Cellar/jq/1.7.1/INSTALL_RECEIPT.json << 'EOF'
{
  "homebrew_version": "4.0.0",
  "installed_on_request": true,
  "installed_as_dependency": false,
  "poured_from_bottle": true,
  "source": {"tap": "homebrew/core"}
}
EOF

zb migrate --dry-run --homebrew-prefix=/tmp/test-homebrew
```

### Platform Note

The development sandbox was Linux x86_64, but Zerobrew targets macOS arm64. The migration infrastructure works correctly (scanning, planning, downloading), but the bottles are for macOS so they don't execute on Linux. This is expected behavior per SPEC.md.

## Issues Encountered and Solutions

### 1. Missing Dev Dependency

**Issue**: Tests used `tempfile::TempDir` but it wasn't in Cargo.toml
**Solution**: Added `[dev-dependencies] tempfile = "3"` to zb_migrate/Cargo.toml

### 2. Export Visibility

**Issue**: `IncompatibleReason` wasn't exported from lib.rs
**Solution**: Added to the `pub use` statement:
```rust
pub use migrate::{IncompatibleReason, MigrationPlan, Migrator, MigrationResult};
```

### 3. Partial Move in Tests

**Issue**: Test called `tab.runtime_dependencies.unwrap()` then tried to use `tab` again
**Solution**: Reordered assertions to call `tab.is_core_formula()` first, then used `.as_ref().unwrap()` for deps

### 4. Unused Imports

**Issue**: Compiler warnings for unused `HashMap` and `Path` imports
**Solution**: Cleaned up imports in scanner.rs

## Progress Callback Integration

The migration leverages the existing `ProgressCallback` system:

```rust
pub type ProgressCallback = Box<dyn Fn(InstallProgress) + Send + Sync>;

pub enum InstallProgress {
    DownloadStarted { name: String, total_bytes: Option<u64> },
    DownloadProgress { name: String, downloaded: u64, total_bytes: Option<u64> },
    DownloadCompleted { name: String, total_bytes: u64 },
    UnpackStarted { name: String },
    UnpackCompleted { name: String },
    LinkStarted { name: String },
    LinkCompleted { name: String },
}
```

This provides consistent progress UI between `zb install` and `zb migrate`.

## Future Enhancements

### Phase 2: Brewfile Support (Not Yet Implemented)

```rust
pub enum BrewfileEntry {
    Tap { name: String, url: Option<String> },
    Brew { name: String, args: Vec<String> },
    Cask { name: String },  // Warn, not supported
    Mas { name: String, id: u64 },  // Warn, not supported
    Vscode { name: String },  // Warn, not supported
}

// Commands:
zb migrate --from-brewfile ~/.Brewfile
zb export --to-brewfile ./Brewfile
```

### Phase 3: Bottle Adoption (Not Yet Implemented)

For users with slow connections, could import existing bottles:
1. Verify bottle SHA matches expected value from API
2. Copy/hardlink bottle tarball to Zerobrew's cache
3. Extract to store (avoiding re-download)

### Phase 4: Services Migration (See SERVICES_FUTURE.md)

Would require implementing full services support first.

## Files Modified/Created

| File | Change |
|------|--------|
| `Cargo.toml` | Added `zb_migrate` to workspace |
| `zb_cli/Cargo.toml` | Added `zb_migrate` dependency |
| `zb_cli/src/main.rs` | Added `migrate` command (+294 lines) |
| `zb_migrate/Cargo.toml` | New crate configuration |
| `zb_migrate/src/lib.rs` | Module exports |
| `zb_migrate/src/tab.rs` | Tab parser (+140 lines) |
| `zb_migrate/src/scanner.rs` | Cellar scanner (+235 lines) |
| `zb_migrate/src/migrate.rs` | Migration logic (+225 lines) |

## Repository

The implementation lives at: https://github.com/chrischabot/zerobrew

This is intended as a development fork. PRs to the upstream `lucasgelfond/zerobrew` will be submitted once the feature is polished.