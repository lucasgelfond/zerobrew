# Services Implementation Status

> **Status**: Foundation Implemented
> **Created**: January 2026  
> **Repository**: https://github.com/chrischabot/zerobrew

## What's Been Implemented

### Core Infrastructure (Complete)

#### New `zb_services` Crate

```
zb_services/
├── Cargo.toml
└── src/
    ├── lib.rs              # Module exports
    ├── error.rs            # Service-specific errors
    ├── definition.rs       # Service definition types from API
    ├── validation.rs       # Security validation
    ├── manager.rs          # High-level service manager
    └── launchd/
        ├── mod.rs
        ├── plist.rs        # Plist generation
        ├── manager.rs      # Launchctl interface
        └── status.rs       # Status parsing
```

#### Implemented Features

| Component | Status | Test Coverage |
|-----------|--------|---------------|
| Service definition parsing | ✓ Complete | 5 tests |
| Plist generation (XML) | ✓ Complete | 6 tests |
| Launchctl manager | ✓ Complete | 2 tests |
| Status parsing (print/list) | ✓ Complete | 4 tests |
| Security validation | ✓ Complete | 6 tests |
| ServiceManager API (start/stop/restart/status) | ✓ Complete | - |
| **Total Tests** | **23 passing** | |

### Security Validations Implemented

- Path traversal prevention (reject `..`, validate absolute paths)
- Executable existence and permission verification
- Nice value range validation (-20 to 19)
- Negative nice requires root validation
- Plist ownership verification
- Symlink attack prevention in LaunchAgents directory
- Keep-alive path validation

### API Enhancements

Added `ApiClient::get_formula_raw()` method to fetch raw Homebrew formula JSON, allowing access to service definitions not included in the `Formula` struct.

## What Remains to Implement

### Phase 1: List and Discovery

```rust
impl ServiceManager {
    pub fn list(&self) -> Result<Vec<ServiceStatus>, ServiceError> {
        // Scan ~/Library/LaunchAgents for homebrew.mxcl.*.plist
        // Check status of each
        // Return all service statuses
    }
}
```

### Phase 2: Enable/Disable (Auto-start)

Requires database extension:

```sql
CREATE TABLE services (
    name TEXT PRIMARY KEY,
    enabled BOOLEAN NOT NULL DEFAULT 0,
    created_at INTEGER NOT NULL
);
```

```rust
impl ServiceManager {
    pub fn enable(&mut self, formula: &str) -> Result<(), ServiceError> {
        // Mark as enabled in database
        // If not running, start it
    }
    
    pub fn disable(&mut self, formula: &str) -> Result<(), ServiceError> {
        // Mark as disabled in database
        // Does NOT stop service, just prevents auto-start
    }
}
```

### Phase 3: CLI Integration

Add to `zb_cli/src/main.rs`:

```rust
#[derive(Subcommand)]
enum Commands {
    // ... existing ...
    
    #[command(subcommand)]
    Services(ServicesCommands),
}

#[derive(Subcommand)]
enum ServicesCommands {
    Start { formula: String },
    Stop { formula: String },
    Restart { formula: String },
    Status { formula: String, #[arg(long)] json: bool },
    List { #[arg(long)] json: bool },
    Enable { formula: String },
    Disable { formula: String },
}
```

### Phase 4: Additional Testing

Integration tests (require macOS):
- Actual launchd start/stop lifecycle
- Service restart on crash (keep_alive)
- Interval services timing verification
- Cron schedule verification
- Log file creation verification

Security tests:
- Race condition testing (concurrent start/stop)
- Resource exhaustion (rapid crashes)
- Formula upgrade during service run

### Phase 5: Error Handling Polish

- Better error messages for common failures
- Graceful handling of formula upgrades
- Service file cleanup on uninstall
- Handle plist file manual deletion

## Known Limitations

1. **macOS Only**: This implementation is launchd-specific
2. **No systemd**: Linux support deferred
3. **Root services**: `require_root: true` services need special handling
4. **Formula compatibility**: Formulas must be installed before service can start

## Testing on Non-macOS Systems

Current tests work on Linux (definition parsing, validation, plist XML generation), but launchctl integration requires macOS. Tests that need macOS are conditionally compiled:

```rust
#[cfg(target_os = "macos")]
mod integration_tests {
    // macOS-specific launchctl tests
}
```

## Build Status

- **Compiles**: Yes
- **All tests pass**: Yes (23 tests)
- **Integration with zerobrew**: Partial (manager implemented, CLI not yet)

## Next Steps

1. Implement `list()` method for discovering all services
2. Add database schema extension for enable/disable state
3. Implement enable/disable methods
4. Add CLI commands with proper error handling
5. Write integration tests (requires macOS)
6. Test on actual macOS system with real formulas
7. Handle edge cases (formula uninstall, plist deletion, etc.)

## Repository

Implementation at: https://github.com/chrischabot/zerobrew

Ready for PR to upstream `lucasgelfond/zerobrew` once CLI integration is complete.