# Merge Queue Setup

## Enable GitHub Merge Queue

1. Go to **Settings** → **Rules** → **Rulesets**
2. Create a new ruleset for your default branch
3. Enable **Require merge queue**
4. Set required status checks:
   - `Ready to Merge`

## Enable Branch Protection

1. Go to **Settings** → **Branches**
2. Add rule for your base branch (e.g., `pr/step-0-1-workspace-ci`)
3. Enable:
   - Require a pull request before merging
   - Require status checks to pass
   - Require merge queue

## Claude Code Review Setup

1. Add `ANTHROPIC_API_KEY` to repository secrets:
   - Go to **Settings** → **Secrets and variables** → **Actions**
   - Add new secret: `ANTHROPIC_API_KEY`

2. Usage in PR comments:
   ```
   @claude please fix the clippy warnings
   @claude add error handling for the edge case where X is empty
   @claude refactor this function to be more readable
   ```

3. Claude will:
   - Read your comment
   - Make the requested changes
   - Commit and push to the PR branch
   - Reply with status

## Auto-Merge Commands

Comment on any PR to approve and queue for merge:

| Command | Effect |
|---------|--------|
| `@claude merge` | Approve and enable auto-merge for this PR |
| `@claude lgtm` | Same as merge |
| `@claude ship it` | Same as merge |
| `@claude approve` | Same as merge |
| `@claude merge all` | Approve and auto-merge ALL open step PRs in order |

PRs will merge automatically once all checks pass. Chained PRs merge in dependency order.

## Workflow Files

- `.github/workflows/ci.yml` - Basic CI (fmt, clippy, test)
- `.github/workflows/merge-queue.yml` - Merge queue validation
- `.github/workflows/claude-review.yml` - Claude Code review bot
- `.github/workflows/claude-merge.yml` - Auto-merge on approval

## Required Repository Settings

For auto-merge to work:

1. **Settings** → **General** → Enable "Allow auto-merge"
2. **Settings** → **Branches** → Add branch protection:
   - Require status checks: `test`, `Ready to Merge`
   - Require approvals: 1 (optional, Claude can approve)
