# Zerobrew Integration Tests

This directory contains integration tests for the zerobrew project.

## Structure

- `common/` - Shared test utilities and helper functions
- `circular_dependency_test.rs` - Comprehensive tests for circular dependency detection

## Running Tests

```bash
# Run all tests (unit + integration)
cargo test

# Run only integration tests
cargo test --test '*'

# Run specific integration test
cargo test --test circular_dependency_test

# Run with output
cargo test -- --nocapture
```

## Writing New Tests

### Integration Tests

Create a new file in `tests/` named `<feature>_test.rs`:

```rust
mod common;

use common::formula;
use zb_core::{resolve_closure, Error};

#[test]
fn my_test() {
    // Test implementation
}
```

### Test Helpers

Add reusable test utilities to `common/mod.rs`:

```rust
pub fn helper_function() -> SomeType {
    // Helper implementation
}
```