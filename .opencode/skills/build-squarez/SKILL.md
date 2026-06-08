---
name: build-squarez
description: Use when running any cargo build, cargo run, or cargo check command in this project. Required before every build — closes the squarez app first, reopens it after.
---

# Build Squarez

## Decision: Always use DEBUG builds during development

- **Debug** = fast compile (~5s), used for all development iteration
- **Release** = slow compile (~35s), only for final distribution — do NOT use unless explicitly asked

## New Task Rule

When the user gives a new task (any message that starts a new piece of work), **kill the squarez process immediately at the start of the task**, before doing any code work. Do not wait until build time.

```bash
pkill squarez || true
```

## Build Rule

Every build follows this exact cycle — no exceptions:

1. **Kill** running squarez app
2. **Build** (debug)
3. **Relaunch** the application

Skipping step 1 can cause resource busy or text file busy errors.
Skipping step 3 means the user has no app to test with — always relaunch.

## Standard build cycle (use this every time on macOS)

```bash
pkill squarez || true; cargo build && ./scripts/macos-bundle.sh && open target/debug/Squarez.app
```

The `&&` ensures squarez only relaunches if the build succeeds.

## Kill only

```bash
pkill squarez || true
```

## Relaunch only (after a successful build)

```bash
open target/debug/Squarez.app
```

## Release cycle (only when explicitly asked)

```bash
pkill squarez || true; cargo build --release && open target/release/Squarez.app
```

## Red Flags — STOP

- About to run `cargo build` without killing first → **kill first**
- About to use `--release` without being asked → **use debug instead**
- Build succeeded but not relaunching → **always relaunch**
- On macOS, make sure to update the bundle via `./scripts/macos-bundle.sh` and launch the `.app` using `open`.

