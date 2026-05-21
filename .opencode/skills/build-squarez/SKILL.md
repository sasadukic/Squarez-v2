---
name: build-squarez
description: Use when running any cargo build, cargo run, or cargo check command in this project. Required before every build — closes squarez.exe first, reopens it after.
---

# Build Squarez

## Decision: Always use DEBUG builds during development

- **Debug** = fast compile (~5s), used for all development iteration
- **Release** = slow compile (~35s), only for final distribution — do NOT use unless explicitly asked

## New Task Rule

When the user gives a new task (any message that starts a new piece of work), **kill squarez.exe immediately at the start of the task**, before doing any code work. Do not wait until build time.

```bash
taskkill /F /IM squarez.exe 2>NUL
```

## Build Rule

Every build follows this exact cycle — no exceptions:

1. **Kill** squarez.exe
2. **Build** (debug)
3. **Relaunch** squarez.exe from `target\debug\squarez.exe`

Skipping step 1 causes "Access is denied" when the linker tries to replace the running executable.
Skipping step 3 means the user has no app to test with — always relaunch.

## Standard build cycle (use this every time)

```bash
taskkill /F /IM squarez.exe 2>NUL; cargo build 2>&1 && start "" "target\debug\squarez.exe"
```

The `&&` ensures squarez only relaunches if the build succeeds.

## Kill only

```bash
taskkill /F /IM squarez.exe 2>NUL
```

## Relaunch only (after a successful build)

```bash
start "" "target\debug\squarez.exe"
```

## Release cycle (only when explicitly asked)

```bash
taskkill /F /IM squarez.exe 2>NUL; cargo build --release 2>&1 && start "" "target\release\squarez.exe"
```

## Red Flags — STOP

- About to run `cargo build` without killing first → **kill first**
- About to use `--release` without being asked → **use debug instead**
- Build failed with "Access is denied" → squarez was still running, kill and retry
- Build succeeded but not relaunching → **always relaunch**
- Using `cmd /c start` or `Start-Process` → **use `start ""` directly**, it's the only form that works in this shell
