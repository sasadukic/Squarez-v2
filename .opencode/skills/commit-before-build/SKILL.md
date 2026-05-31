# Commit Before Build

## Rule

**Before EVERY build, commit the current work to git.**

No exceptions. This prevents losing work, makes bisect possible, and keeps a clean history of iterative changes.

## When This Applies

- `cargo build`
- `cargo run`
- `cargo check`
- Any command that compiles the project

## Steps Before Every Build

1. **Check git status** — see what's changed
2. **Stage relevant files** — `git add -A` or selective add
3. **Commit with a descriptive message** — use conventional commits format
4. **Now build**

## Message Format

```
<type>: <description>
```

Types:
- `feat:` — new feature or behavior
- `fix:` — bug fix
- `refactor:` — code restructuring with no behavior change
- `chore:` — maintenance, cleanup, dependency updates
- `wip:` — work in progress checkpoint

For iterative development (multiple builds in a session), use `wip:` with a number:
```
wip: pencil pixel-perfect iteration 3
```

## Why

- If the build breaks something, `git reset --hard HEAD~1` gets you back instantly
- If you need to compare before/after, `git diff HEAD~1` shows exactly what changed
- The user can see a granular history of what happened
- Prevents the "it was working 5 minutes ago" panic

## Example

```bash
# BAD — build without committing
cargo build

# GOOD — commit first
git add -A
git commit -m "wip: pixel-perfect pencil L-shape removal"
cargo build
```
