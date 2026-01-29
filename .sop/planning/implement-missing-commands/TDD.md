# TDD Guidelines for Update/Outdated/Upgrade Commands

**Read this document before starting any implementation step.**

## RFC 2119 Keywords

The keywords "MUST", "MUST NOT", "SHALL", "SHALL NOT", "SHOULD", "REQUIRED" 
in this document are to be interpreted as described in [RFC 2119](https://www.ietf.org/rfc/rfc2119.txt).

## TDD Cycle: Red, Green, Refactor

### 1. Red: Write a Failing Test First

Before writing any implementation code:

1. Write a test that describes the expected behavior
2. Run `cargo test` - the test **must fail**
3. If the test passes, you either:
   - Wrote the wrong test
   - The feature already exists

Example for Step 1 (ApiCache.clear):
```rust
#[test]
fn clear_removes_all_entries() {
    let cache = ApiCache::in_memory().unwrap();
    
    // Add entries
    cache.put("https://example.com/a.json", &entry()).unwrap();
    cache.put("https://example.com/b.json", &entry()).unwrap();
    
    // Clear and verify
    let removed = cache.clear().unwrap();
    assert_eq!(removed, 2);
    assert!(cache.get("https://example.com/a.json").is_none());
}
```

### 2. Green: Write Minimal Code to Pass

1. Write the **simplest** code that makes the test pass
2. Don't optimize or add extra features
3. Run `cargo test` - the test **must pass**

### 3. Refactor: Clean Up

1. Improve code quality without changing behavior
2. Remove duplication
3. Run `cargo test` - tests **must still pass**

## Project Test Patterns

Follow existing patterns in the codebase:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    // Use in_memory() for isolated tests
    #[test]
    fn test_name_describes_behavior() {
        let cache = ApiCache::in_memory().unwrap();
        // arrange, act, assert
    }
}
```

## Running Tests

```bash
# Run all tests
cargo test --workspace

# Run specific test
cargo test clear_removes_all_entries

# Run tests for specific crate
cargo test -p zb_io
```

## Checklist Before Each Step

- [ ] Read the step requirements in `implementation/plan.md`
- [ ] Write failing test(s) first
- [ ] Verify test fails with `cargo test`
- [ ] Implement minimal code
- [ ] Verify test passes
- [ ] Refactor if needed
- [ ] Verify all tests still pass

---

## Step Completion Gate (REQUIRED)

After completing each implementation step, the following gate process MUST be followed. 
The implementer SHALL NOT proceed to the next step without explicit user approval.

### 1. Commit and Push (REQUIRED)

After all tests pass, the implementer MUST:

```bash
git add -A
git commit -m "feat(commands): step N - <description>"
git push
```

### 2. Manual Testing Instructions (REQUIRED)

The implementer MUST provide the user with manual testing instructions specific to the completed step. Format:

```
## Manual Testing for Step N

### Prerequisites
<any setup needed>

### Test Commands
<exact commands to run>

### Expected Results
<what the user should see>
```

### 3. User Approval Gate (REQUIRED)

The implementer MUST ask the user:

> **Step N complete.** Please verify:
> 1. Review the commit: `git show HEAD`
> 2. Run the manual tests above
> 3. Confirm to proceed to Step N+1, or request changes
>
> **Reply "continue" to proceed or describe any issues.**

The implementer SHALL NOT proceed until the user explicitly replies with approval.

### 4. Prohibited Actions

The implementer MUST NOT:
- Automatically continue to the next step
- Batch multiple steps into one commit
- Skip the manual testing instructions
- Skip the user approval gate
