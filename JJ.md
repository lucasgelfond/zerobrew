# JJ.md — Jujutsu (jj) Workflow Notes

This is a short, self-reminder manual for using jj in this repo.

## Policy

- Always run jj commands with explicit approval/escalation.
- Use jj for all VCS operations (no git commands directly).

## Common Tasks

### Create a new change for a checklist step

```
jj new -m "Step X.Y: <short title>"
```

### Update the current change description

```
jj describe -m "Step X.Y: <short title>"
```

### Check status and history

```
jj status
jj log -n 5
```

### Create or update a bookmark for a PR branch

```
jj bookmark create pr/step-X-Y -r @
jj bookmark set pr/step-X-Y -r @
```

### Push a PR branch

```
jj git push --bookmark pr/step-X-Y
```

### Edit an older change

```
jj edit <change-id-or-revset>
```

### Rebase a child onto an updated change

```
jj rebase -s <child> -d <updated-parent>
```

## Checks (per PR.md)

```
cargo fmt
cargo clippy -- -D warnings
cargo test
```
