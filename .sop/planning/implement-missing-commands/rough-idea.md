# Rough Idea

**Source:** [GitHub Issue #71 - lucasgelfond/zerobrew](https://github.com/lucasgelfond/zerobrew/issues/71)

## Summary

Implement five missing subcommands for zerobrew (a Homebrew alternative):

1. **`outdated`** - List packages that have newer versions available
2. **`upgrade`** - Upgrade installed packages to their latest versions
3. **`update`** - Update the package index/formulae
4. **`cleanup`** - Remove old versions and cached files
5. **`doctor`** - Diagnose and report system/installation issues

## Original Issue

```
~ $ zb outdated
error: unrecognized subcommand 'outdated'

~ $ zb upgrade
error: unrecognized subcommand 'upgrade'

~ $ zb update
error: unrecognized subcommand 'update'

~ $ zb cleanup
error: unrecognized subcommand 'cleanup'

~ $ zb doctor
error: unrecognized subcommand 'doctor'
```

These commands are expected by users familiar with Homebrew but are currently not implemented in zerobrew.
