# Project Summary: Zerobrew Update/Outdated/Upgrade Commands

## Overview

This planning project transformed GitHub Issue #71 (request for `outdated`, `upgrade`, `update`, `cleanup`, and `doctor` commands) into a detailed design and implementation plan for zerobrew.

## Artifacts Created

```
.sop/planning/implement-missing-commands/
├── rough-idea.md                    # Original issue capture
├── idea-honing.md                   # Requirements Q&A (9 questions)
├── design/
│   └── detailed-design.md           # Full technical design
├── implementation/
│   └── plan.md                      # 10-step implementation checklist
├── TDD.md                           # TDD guidelines (read before each step)
└── summary.md                       # This document
```

## Before Starting Implementation

**Always read `TDD.md` first.** This project follows Red-Green-Refactor:
1. Write failing test
2. Write minimal code to pass
3. Refactor

## Scope Decision

**Phase 1 (this design):** `update`, `outdated`, `upgrade` - the core workflow
**Phase 2 (future):** `cleanup`, `doctor`

## Key Design Decisions

| Decision | Choice | Rationale |
|----------|--------|-----------|
| Homebrew compatibility | Yes | Ease adoption, script compatibility |
| Version comparison | Hash-based (sha256) | Simpler, catches rebuilds |
| `update` behavior | Clear API cache | Aligns with zerobrew's stateless design |
| Concurrency | Parallel API checks | Consistent with zerobrew's performance focus |
| Error handling | Warn and continue | Better UX for batch operations |
| Upgrade scope | All or specific | Matches Homebrew behavior |

## Implementation Overview

**10 incremental steps:**

1. ApiCache management methods (clear, stats)
2. `update` command
3. OutdatedPackage type and detection logic
4. `outdated` command (basic)
5. `outdated` output formats (quiet, verbose, json)
6. `upgrade` command (single package)
7. `upgrade` command (all packages)
8. Dry-run support
9. Integration tests
10. Documentation

Each step builds on previous work and results in demoable functionality.

## Technical Highlights

- **No new dependencies** - uses existing infrastructure (rusqlite, reqwest, futures)
- **Reuses existing patterns** - parallel fetching, progress callbacks, error handling
- **Minimal API surface** - 3 new CLI commands, ~5 new methods on existing types

## Next Steps

1. Review the detailed design at `.sop/planning/design/detailed-design.md`
2. Review the implementation plan at `.sop/planning/implementation/plan.md`
3. Begin implementation following the checklist
4. Consider Phase 2 (`cleanup`, `doctor`) after Phase 1 is complete

## Open Questions for Implementation

- Should `update` also clear the blob cache, or just API cache?
- Should `upgrade` run `gc` automatically after upgrading?
- What's the desired behavior if a package's dependencies change between versions?

These can be resolved during implementation based on Homebrew's behavior or user preference.
