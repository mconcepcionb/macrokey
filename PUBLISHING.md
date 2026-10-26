# Publishing macrokey

This directory is currently a project inside the `personal` monorepo and is not
tracked there. Publish it as its own repository so the history, license, CI and
releases are self-contained.

## Pre-flight

- [x] `cargo fmt --check`
- [x] `cargo clippy --all-targets -- -D warnings`
- [x] `cargo test` (7 unit tests)
- [x] `vendor/`, `logs/`, `target/`, generated schemas and local databases are
      git-ignored
- [x] No secrets, tokens or personal paths in tracked source
- [x] MIT `LICENSE`, `SECURITY.md`, `CHANGELOG.md`
- [x] CI workflow at `.github/workflows/build.yml`
- [x] Screenshots in `docs/media/`
- [ ] Decide the final repository name and Tauri identifier
- [ ] Create the standalone repository
- [ ] Sign the release binary (optional; not done yet)

## 1. Create a standalone repository

Run this from `macrokey/`, not from the parent `personal/` directory:

```powershell
cd <path-to-macrokey>

git init
git add .
git status --short          # check that vendor/, logs/, target/, *.db are absent
git commit -m "Initial public release: macrokey 0.1.0"
git branch -M main
git remote add origin git@github.com:<you>/macrokey.git
git push -u origin main
```

A `.git` directory inside `macrokey/` makes it a nested repository; the parent
`personal` repository will continue to ignore it as an untracked directory. Do
not `git add macrokey/` from the parent unless you deliberately want a subtree.

Before committing, confirm the ignored files are not staged:

```powershell
git ls-files | Select-String 'vendor/|logs/|target/|\.db$|gen/schemas'
```

There should be no output.

## 2. Build the release binary

```powershell
cd agent\src-tauri
cargo build --release
```

Artefact:

```
agent/src-tauri/target/release/macrokey-agent.exe
```

The frontend (`agent/ui`) and the seed YAML are embedded into the executable, so
the release is a single portable `.exe`. There is no installer yet; users put it
where they like and create a shortcut.

CI builds the same artefact on every push and uploads it as a workflow artifact.
To publish a GitHub Release:

```powershell
git tag v0.1.0
git push origin v0.1.0
```

Then attach `macrokey-agent.exe` to the release (the workflow artifact can be
downloaded and re-uploaded), and copy the relevant section of `CHANGELOG.md`.

## 3. Repository metadata

Suggested description:

> Context-aware layers, gestures and an on-screen legend for a cheap 3-key + 1-knob CH57x macropad.

Suggested topics: `macropad`, `ch57x`, `rawinput`, `tauri`, `rust`, `windows`,
`productivity`.

Keep `README.md` as the entry point; it contains the quick start, flashing
instructions, screenshots and hardware scope. `agent/README.md` is the detailed
developer/operator guide.

## 4. Release caveats to state in the release notes

- Windows 10/11 x64 + WebView2 (preinstalled on Windows 11; Windows 10 users may
  need the Evergreen Runtime).
- Supports **only** the `1189:8890` 3-key + 1-knob CH57x board with the provided
  `config/pad-single-layer.yaml` flashed. Other layouts need a different
  firmware profile and UI changes.
- The pad must be flashed once with `ch57x-keyboard-tool` + USBDK; the agent does
  not flash firmware.
- The binary is unsigned; SmartScreen may warn. There is no auto-updater.
- No network access, no telemetry. The database and log stay local.
- The settings screenshot uses a generic `%APPDATA%\macrokey\macrokey.db`
  label. Keep it generic when refreshing screenshots.

## 5. After publishing

- Watch the first issues for WebView2/tray problems and include the
  `agent.log` instructions.
- Keep `Cargo.lock` committed.
- Run `cargo test`, `cargo clippy --all-targets -- -D warnings` and
  `cargo fmt --check` before each release.
- Back up `%APPDATA%\macrokey\macrokey.db` (or use Export) before a release that
  changes the schema.
