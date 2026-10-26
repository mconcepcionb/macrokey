# Gap closure roadmap

Living plan for everything between the published v0.1 prototype and a trustworthy
1.0. It complements [review.md](review.md), which records the audit findings and
the fixed blockers.

Started: 2026-10-26. Baseline: public v0.1 prototype, portable Windows binary,
green CI, 7 unit tests.

## How to use this plan

- Work one batch per pull request. Do not mix a database migration with packaging
  or a UI refactor.
- Every batch must pass `cargo fmt --check`,
  `cargo clippy --all-targets -- -D warnings`, `cargo test`, and the relevant
  manual tests below.
- Database changes must preserve existing data and leave a backup. Never ship a
  downgrade path that silently overwrites a newer schema.
- Engine/OSD changes need a physical-pad pass before merge; CI cannot press the
  pad.
- Code signing and certificates stay outside the repository; CI gets them from
  secrets only.

Priority: **P0** blocks a trustworthy release; **P1** important for the next
public release; **P2** polish/deferred; **P3** out of scope for now.

Effort: **S** under half a day, **M** one to two days, **L** more than two days.

Risk: likelihood of regressions or hardware/OS-specific surprises.

## A. Core correctness and data safety

| ID | Item | Priority | Effort | Risk | Depends on | Acceptance |
|---|---|---|---|---|---|---|
| A1 | **Schema migrations + backup** | P0 | M | Medium | — | `meta.schema` is read, not overwritten. A v1 database migrates to v2; rows survive; a backup is written before migration; a newer schema is refused with a clear message; migration test added. |
| A2 | **One-shot semantics** | P1 | S | Medium | Tests | A one-shot layer is consumed by the next input **down**, not when its action eventually runs. A delayed double/hold still uses the one-shot layer for that same gesture; a different key never sees it. Manual pad pass + tests. |
| A3 | **Pause pad actions while Settings is focused** | P1 | S | Low | — | **Implemented in this PR.** Transient state (pressed keys, pending taps, overlay, one-shot, peek) is cleared; input is ignored while Settings is foreground; Preview and tray legend still work; leaving Settings resumes normal action. |
| A4 | **Gesture engine tests** | P0 | L | Medium | Refactor | Extract gesture decisions from Tauri/OSD/actions into a testable core with a fake effects sink. Cover: tap only, double only, tap+double, hold only, hold+double, expired pending tap, auto-repeat, knob acceleration, layer switch/back/next/prev, one-shot, momentary hold, focus switch, reload. |
| A5 | **Pad presence/health** | P2 | S/M | Low | — | On startup/after resume, indicate whether the pad has been seen. A missing pad does not kill the tray app. Tray/tooltip or settings status can say "no pad seen yet"; diagnostics logged once. |

## B. UX and features

| ID | Item | Priority | Effort | Risk | Depends on | Acceptance |
|---|---|---|---|---|---|---|
| B1 | **YAML import** | P1 | S/M | Medium | — | Settings has an Import action; file is read locally, parsed, validated, confirmed (overwrites current config), saved in a transaction, and the UI/engine reload. Malformed YAML and invalid layers/rules show a toast and leave the old config untouched. Script actions are called out in the confirmation. |
| B2 | **Opt-in autostart** | P1 | S | Low | — | Settings has "Start with Windows"; writes/removes only `HKCU\Software\Microsoft\Windows\CurrentVersion\Run\macrokey`; default off; status persists; no admin required; `reg query` test documented. |
| B3 | **Per-game Steam contexts** | P2 | Data | Low | User decision | Rules for game executables + per-game contexts. No code change unless rule matching needs more fields. Previously deferred by the user. |
| B4 | **Keyboard accessibility** | P2 | M | Low | — | Full tab order, visible focus rings, modal focus trap/return, Enter/Space on cards, ARIA labels for icon-only controls. Audit with keyboard only. |
| B5 | **Localization** | P2 | M | Low | — | Strings externalized; Spanish first (user locale), English fallback; no layout overflow in the settings deck. |
| B6 | **Onboarding/help** | P2 | S | Low | — | First-run checklist: flash the pad, find the tray icon, open Settings. Explain the F13–F18 filter and what to do when the pad is silent. Link troubleshooting. |

## C. Security and dependencies

| ID | Item | Priority | Effort | Risk | Depends on | Acceptance |
|---|---|---|---|---|---|---|
| C1 | **Externalize UI JS/CSS + CSP** | P1 | M | Medium | — | Inline `<script>` and `<style>` move to bundled files. CSP: `default-src 'self'`, `script-src 'self'`, `style-src 'self' 'unsafe-inline'`, `connect-src ipc: http://ipc.localhost`, `img-src 'self' data:`. Settings IPC and Rust `eval`-driven OSD still work under CSP. Browser preview still works. |
| C2 | **Replace `serde_yaml`** | P1 | S | Low | — | Use a maintained YAML crate (`serde_yaml_ng` or `serde_yml`); seed, export, round-trip and import tests pass; YAML output stays readable and version 3-compatible. |
| C3 | **Dependency audit policy** | P1 | S | Low | — | Add `cargo audit` (or `cargo deny`) to CI with an explicit ignore for the Linux-only `glib` advisory and a link to this plan. Keep Dependabot alerts on; document dismissal reasons. |
| C4 | **Log privacy and levels** | P2 | S | Low | — | Rotation already done. Decide whether window titles are needed in release logs; optionally redact titles or add a diagnostic toggle. Document exactly what is logged. |
| C5 | **Validate layer colors** | P2 | S | Low | — | Layer colors normalize to `#rrggbb` (fallback `#7dd3fc`). No arbitrary string reaches a `style` attribute or CSS variable. |
| C6 | **Config trust warning** | P2 | S | Low | B1 | Import/export UI says a config can run scripts and open URLs; exported files are executable content. |

## D. Packaging and release

| ID | Item | Priority | Effort | Risk | Depends on | Acceptance |
|---|---|---|---|---|---|---|
| D1 | **Installer** | P1 | M | Medium | Icons | NSIS installer built in CI; installs per-user or per-machine consistently; includes a WebView2 bootstrapper/check; uninstall removes the app but not the user DB without asking. Portable exe remains available. |
| D2 | **Release automation + checksums** | P1 | S | Low | D1 | Pushing a `v*` tag creates a GitHub Release with exe, installer, SHA-256 checksums and release notes; artifacts come from the same commit. |
| D3 | **Code signing** | P0 | M | Low | Certificate | OV/EV certificate configured through CI secrets; `signtool` verifies exe and installer; SmartScreen reputation starts accumulating. No certificate material in git. |
| D4 | **Auto-updater** | P2 | M | Medium | D3 | Signed update manifests; opt-in or silent-check setting; rollback path; never updates across an incompatible schema. |
| D5 | **WebView2 detection** | P1 | S/M | Low | D1 | Installer checks/installs the Evergreen Runtime; portable mode documents the dependency and shows a readable error, not a blank window. |
| D6 | **Portable release** | Done | — | — | — | `macrokey-agent.exe` in CI; keep it as the no-install option. |

## E. Risky or deferred

| ID | Item | Priority | Effort | Risk | Notes |
|---|---|---|---|---|---|
| E1 | **Suppress F13–F18** | P2 | M | **High** | Options: global low-level keyboard hook (no device filter) or Interception driver (needs install). Could swallow F13–F18 from every keyboard and interact with security software. Needs a fail-safe and a live test matrix. Deferred until the core is stable. |
| E2 | **Multi-device/generic pads** | P3 | L | Medium | The UI and firmware profile are specific to 3 keys + 1 knob. Generalizing needs a layout model and per-device flashing docs. |
| E3 | **Custom firmware/QMK** | P3 | L | High | Out of scope; the current design deliberately avoids custom firmware. |

## F. Verification

### CI gates

- `cargo fmt --check`
- `cargo clippy --all-targets -- -D warnings`
- `cargo test`
- `cargo build --release`
- `cargo audit` with documented ignores (C3)
- Installer build on tags (D1)
- Artifact/checksum generation on tags (D2)

### Manual matrix (before each public release)

- [ ] Flash the pad from a clean machine with `pad-single-layer.yaml`; verify F13–F18.
- [ ] Tap, double, hold on k1/k2/k3 and knob press; CCW/CW with acceleration.
- [ ] Double-only and tap+double bindings; expired double window.
- [ ] Momentary hold + another key; one-shot + tap/double/hold; layer next/prev/back.
- [ ] Context switch while a key is held; context switch during a pending tap.
- [ ] Settings focused: pad does nothing; Preview works; leaving Settings resumes.
- [ ] OSD primary/focused monitor, 100%/150% DPI, multi-monitor, taskbar work area.
- [ ] Sleep/resume, lock/unlock, explorer restart, app restart, second launch.
- [ ] Elevated target app: action failure is logged and the app stays stable.
- [ ] YAML export/import round-trip; malformed import leaves the DB untouched.
- [ ] DB migration from the previous release; backup exists; revision reload works.
- [ ] Autostart on/off survives reboot.
- [ ] Installer install/uninstall/upgrade; portable exe still runs.

### Release gate

- [ ] Version + changelog updated.
- [ ] Screenshots current and privacy-safe.
- [ ] LICENSE, SECURITY, roadmap links current.
- [ ] Signed binaries and checksums.
- [ ] Release notes list hardware scope, Windows/WebView2 requirements, flashing, known limitations.
- [ ] Rollback: previous exe/installer and DB backup instructions.

## Sequence

### Batch 0 — this PR
- Roadmap (this file).
- A3 Settings-focus pause.

### Batch 1 — hardening
- A1 schema migrations + backup.
- A2 one-shot semantics.
- C2 YAML dependency replacement.
- C3 dependency audit policy.
- B1 YAML import.
- B2 autostart.
- Tests/docs for all of the above.

### Batch 2 — security and packaging
- C1 CSP/UI externalization (may split into two PRs).
- D1 installer.
- D5 WebView2 detection.
- D2 release automation/checksums.
- D3 signing as soon as the certificate exists.

### Batch 3 — test depth and polish
- A4 engine core tests.
- F2 UI smoke tests.
- A5 pad presence/health.
- B4 accessibility, B5 localization, B6 onboarding.
- C4/C5/C6.

### Deferred / needs a decision
- E1 F13–F18 suppression.
- B3 per-game Steam.
- D4 auto-updater (after signing).
- E2/E3.

## Open decisions

1. **Code signing**: which certificate (OV vs EV), provider, and CI secret mechanism?
2. **Installer**: NSIS (smaller, recommended) vs WiX/MSI (enterprise deployment)?
3. **F13–F18 suppression**: accept a global hook, require Interception, or leave documented?
4. **Per-game Steam**: which games/contexts first?
5. **Localization**: Spanish first, or English-only for now?
6. **Next release number**: `0.1.1` for hardening only, `0.2.0` if import/autostart land first?
