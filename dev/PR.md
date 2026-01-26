# PR / Branch Workflow Instructions (for Codex + Agents)

Goal: implement Zerobrew v0 via **small, reviewable PRs** that follow the checklist in `Zerobrew v0 — Iterative Build Plan`.  
Each step should land as its own PR (or at most 2–3 steps per PR if they’re inseparable).

**Strong preference:** create **stacked diffs** using `jj` so PRs build on top of each other cleanly.

---

## 0) Rules of engagement

- **One checklist step = one PR** (default).
- PRs must be:
  - small (ideally < ~500 LOC diff),
  - self-contained,
  - fully tested,
  - only touching files relevant to that step.
- Every PR must include:
  - new/updated unit tests,
  - `cargo fmt`,
  - `cargo clippy -- -D warnings`,
  - `cargo test`.

No PR is “done” unless all checks pass.

---

## 1) Branching & stacked PR strategy (jj-first)

Use `jj` to maintain a linear stack of commits, then push each commit as its own Git branch + PR.

### 1.1 Initial setup (once)
- Create a single base branch: `main` (already exists).
- Create a working branch stack root:
  - `jj new main -m "stack root"`

### 1.2 For each checklist step
For step `X.Y`, do:

1) **Create a new change on top of current tip**
   - `jj new -m "Step X.Y: <short title>"`

2) Implement only that step:
   - small modules
   - tests per file
   - no drive-by changes

3) Run checks:
   - `cargo fmt`
   - `cargo clippy -- -D warnings`
   - `cargo test`

4) Commit message format (required):
   - `Step X.Y: <verb phrase>`
   Examples:
   - `Step 1.2: Add deterministic resolver`
   - `Step 3.3: Implement bounded parallel downloader`

5) **Push as its own branch**
   Create a git branch pointing to this change and push:
   - `jj git branch create pr/step-X-Y`
   - `jj git push --branch pr/step-X-Y`

6) **Open PR targeting `main`**
   PR title:
   - `[X.Y] <short title>`
   PR body must include:
   - link to the checklist item text
   - acceptance criteria + how it was tested
   - any follow-ups explicitly deferred

### 1.3 Stacked diffs expectation
- Each PR should be based on the previous PR’s commit.
- When you open PR `X.Y`, GitHub will show it as a diff against `main`, but reviewers should understand it’s part of a stack.
- In the PR description, add:
  - “Stacked on top of: PR #[previous]”
  - “Next PR in stack: PR #[next] (once available)”

If you can, use GitHub’s “stacked PR” linking conventions (manual cross-links are fine).

---

## 2) Updating the stack after review feedback

When changes are requested on PR `X.Y`:

1) Checkout that change:
   - `jj edit <change-id>` (or select by description)

2) Apply fixes, re-run checks.

3) Force-push updated branch:
   - `jj git push --branch pr/step-X-Y --force-with-lease`

4) Rebase descendants (if needed):
   - `jj rebase -s <descendant> -d <updated-change>`
   - Then `jj git push` the downstream PR branches (force-with-lease).

**Important:** never “fix forward” in later PRs. Fix the PR that introduced the issue.

---

## 3) PR checklist template (must paste into each PR)

**Summary**
- Implements checklist step: X.Y
- Description: …

**Acceptance criteria**
- [ ] <criterion 1>
- [ ] <criterion 2>
- …

**Tests**
- `cargo test`
- `cargo clippy -- -D warnings`
- `cargo fmt`
- Added/updated: <list unit tests>

**Notes / Follow-ups**
- <explicitly list anything deferred>

---

## 4) Naming conventions

Branches:
- `pr/step-0-1-workspace-ci`
- `pr/step-1-2-resolver`
- `pr/step-3-3-parallel-downloader`

PR titles:
- `[1.2] Deterministic dependency resolver`

Commit messages:
- `Step 1.2: Add deterministic resolver`

---

## 5) Guardrails to keep code clean

- Keep module public APIs tiny.
- Typed errors, no `anyhow` at module boundaries.
- Pure logic in `zb_core`, I/O in `zb_io`.
- Every file has its own tests.
- Avoid introducing framework-y abstractions.

---

## 6) Delivery expectations

Create PRs for steps in order starting at **Step 0.1**.
Open PRs as soon as a step is complete, even if later steps aren’t started yet.

The end state should be a stack of PRs that I (the human) can review one-by-one and merge in order.
