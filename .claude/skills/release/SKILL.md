---
name: release
description: Build and package a release of retype (version bump, changelog, release build, SHA-256, tag). Only when the user explicitly runs /release.
argument-hint: "[version, e.g. 0.2.0]"
disable-model-invocation: true
---

# Release $ARGUMENTS

1. Preconditions: clean `git status`, on `main`, `/verify` green,
   `/security-audit` verdict "ready". Stop and report otherwise.
2. Set `version = "$ARGUMENTS"` in `[workspace.package]` of the root `Cargo.toml`.
3. Add a `CHANGELOG.md` section from `git log <last tag>..HEAD --oneline`,
   grouped into Added / Changed / Fixed. Show it to the user for approval.
4. Build: `cargo build --release -p retype` (on Windows, MSVC toolchain).
5. Hash: `certutil -hashfile target\release\retype.exe SHA256` (Windows) or
   `sha256sum` elsewhere; write it to `target/release/retype.exe.sha256`.
6. Commit `chore(release): v$ARGUMENTS` and create tag `v$ARGUMENTS`.
7. **Ask before pushing** the commit and tag. CI attaches the artifacts.
