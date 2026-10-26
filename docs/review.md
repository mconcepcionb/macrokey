# Code review and release readiness — 2026-10-26

## Verdict

**Ready to publish as a v0.1 public prototype**, provided the standalone-repo
steps in [PUBLISHING.md](../PUBLISHING.md) are followed.

It is **not yet a 1.0 / mass-market release**:

- the binary is unsigned and has no installer, autostart or auto-updater;
- the hardware scope is one exact CH57x board (`1189:8890`) and requires a
  one-time flash with an external tool;
- the engine gesture state machine and the WebView UI have no automated tests;
- the database schema has a version field but no migration framework;
- the WebView CSP is unset (the UI is local-only, but this would matter if remote
  content were ever loaded).

## Evidence

- `cargo fmt --check` — clean.
- `cargo clippy --all-targets -- -D warnings` — clean.
- `cargo test` — 7 tests pass (`config` rules/validation/round-trips,
  `db` SQLite round-trip, action key parsing, tray text shortening).
- Live debug-build checks:
  - fresh database → settings window opens by itself; DB is seeded; `agent.log`
    is written next to the DB;
  - second launch → exits, focuses the existing settings window, one process
    remains;
  - UI screenshots generated from the real layout and a browser HUD preview.
- Source review of all Rust modules, the SQLite schema, the Raw Input path and the
  settings UI escaping.

## Findings fixed in this pass

| Severity | Finding | Fix |
|---|---|---|
| High | Two agents could run at once, doubling input and racing the database. | Official `tauri-plugin-single-instance`; second launch focuses Settings and exits. |
| High | A release build is tray-only with no visible first-run surface and logs were invisible. | First launch opens Settings; append-only `agent.log` next to the DB; startup/load/Input errors show a message box. |
| High | `SendInput` failures (e.g. elevation/UIPI) were silent. | `SendInput` return value is checked; failures are logged. |
| High | Double-only bindings never fired; an expired pending tap could be lost between ticks. | First tap opens the double window even without a tap action; expired pending taps are flushed before the next press. |
| High | An active one-shot layer could hide a hold layer switch and leave the tray stale. | Layer holds consume the one-shot; one-shot consumption updates the tray immediately. |
| Medium | OSD hide had a race and blocked the engine thread for 180 ms on context changes. | Generation-guarded, asynchronous hide; show claims its generation before window work. |
| Medium | UTF-16 text could be split between a surrogate pair. | Chunks never end on a high surrogate. |
| Medium | Missing `fmt`/`clippy` cleanliness and only 3 tests. | Cleaned lints, 7 tests, CI. |
| Low | The log grew without bound. | Rotates to `agent.log.1` at 2 MB. |
| Low | `mutex.lock().unwrap()` on input/legend paths could panic if poisoned. | Non-panicking lock handling. |
| Low | Capabilities description claimed no webview IPC. | Corrected description; local UI only. |

## Remaining risks and recommended next steps

Priority order:

1. **Code signing.** Unsigned binaries trigger SmartScreen. A signing
   certificate is the single biggest trust improvement; otherwise state it
   clearly in the release notes.
2. **Installer / autostart.** A portable `.exe` is acceptable for v0.1. Tauri
   bundling (NSIS/WiX) and an opt-in autostart entry would make it feel like a
   real tray app.
3. **Engine/UI tests.** The gesture engine is the highest-value untested area:
   double-only, hold + one-shot, rotation acceleration, context switches during a
   pending tap. Extract the pure decision logic or add a headless engine test
   harness.
4. **Schema migrations.** `meta.schema` is written but never checked. Before the
   first breaking config change, add a migration table and a backup-before-upgrade
   step. Export remains the manual backup path.
5. **CSP and UI hardening.** Move inline scripts/styles to files and set a CSP if
   the UI will ever render remote content. Today it loads only bundled assets.
6. **Suppressing `F13`–`F18`.** The phantom keys still reach the focused app.
   Most apps ignore them, but a low-level hook or an Interception driver would be
   needed for completeness.
7. **Settings-focus behavior.** While the Settings window is foreground, pad
   actions still execute into that window. The Preview button is the safe way to
   test. Consider pausing actions (while still showing the HUD) when the agent's
   own window is focused.
8. **Logging hygiene.** The log rotates at 2 MB, but it still contains window
   titles used for rule matching. Mention that in bug-report instructions.
9. **Dependency hygiene.** `serde_yaml` 0.9 is effectively unmaintained; consider
   `serde_yml`/`serde_yaml_ng` at the next dependency refresh. Run `cargo audit`
   before a release.
10. **YAML import.** Export exists; import is still missing. That is fine for v0.1
    but limits sharing profiles.
11. **Hardware scope.** The README must keep saying that this is not a generic
    macropad app: it targets one board and one flashed profile.
12. **Per-game Steam contexts.** Currently one `steamwebhelper.exe` context; games
    are separate processes.
13. **One-shot edge cases.** A one-shot layer is consumed when an action actually
    runs. A double/hold key can leave it active during its gesture window, so a
    second key can also see it. Define and test the intended "next key only"
    semantics for delayed gestures.

## Security review

- No network code, no telemetry, no credential storage.
- Raw Input is filtered by the pad's VID/PID; normal keyboard input is not
  captured.
- SQLite access is parameterized; table/column names are static.
- UI interpolation escapes HTML/attributes. The layer colour is inserted into a
  `style` attribute; it is local data, but validating it as `#rrggbb` would be a
  small hardening win.
- `script` actions intentionally run `cmd /C` as the user; the config is
  executable content and should be treated as such.
- Unsafe blocks were reviewed: Raw Input buffer sizes, `SendInput`, `ShellExecute`,
  window/process queries. No unsoundness found; the sizes passed to Win32 match
  the allocated buffers.

## Release gate

For a **v0.1 public prototype**, all of the following are true:

- clean `fmt`, `clippy`, tests;
- single-instance, first-run, logging and visible fatal errors;
- license, security policy, changelog, publishing checklist;
- CI that builds a portable Windows binary;
- screenshots and a user-facing README quick start.

The remaining items above are quality/trust improvements, not blockers for
sharing the prototype.
