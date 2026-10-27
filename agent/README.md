# macrokey agent

Windows host agent for the 3-key + knob pad. The pad is flashed once to emit
`F13`–`F18`; everything else — contexts, layers, gestures, actions, OSD — is
data in a local SQLite database and edited from the app.

```
Raw Input (pad only) ─┐
focus watcher ────────┴─> context -> layer -> binding -> SendInput / launch
                                      │
                                      └─> OSD (click-through, context · layer)
```

## The model

- A **context** is an environment: `General` (fallback), `VS Code`, `opencode`,
  `VLC`, `Steam`. Rules map the focused process/title to a context.
- A context owns an **ordered list of layers** and one default layer. VS Code,
  for example: `Edit` (default) → `Nav` → `Debug` → `Build`.
- **Layers are a shared library.** `Nav` is the same layer in General, VS Code
  and opencode; edit it once.
- A **binding** is per input and gesture: `tap`, `double`, `hold`.
- A **layer action** can switch layer, apply a layer to the next key only,
  keep a layer while held, cycle next/prev inside the context, or return to the
  context default.

Nothing about a context or a layer is compiled into the agent.

## Run

```powershell
cd agent/src-tauri
cargo run
```

First launch creates `%APPDATA%\macrokey\macrokey.db`, seeds it from the embedded
copy of `config/agent.draft.yaml`, and **opens Settings once**. After that the app
is tray-only: **Settings…**, **Show current layer** and **Quit** live in the tray
icon, and closing Settings hides it.

A second launch of the executable does not create a second agent; it focuses the
running instance's Settings window and exits. Logs are appended to
`agent.log` next to the database (`%APPDATA%\macrokey\agent.log` by default).

| Environment | Meaning |
|---|---|
| `MACROKEY_DB` | use another database file (and put `agent.log` beside it) |
| `MACROKEY_SETTINGS=1` | open the settings window at startup |
| `MACROKEY_DEMO=1` | show a sticky demo OSD (no pad needed) |
| `--db <path>` | database file (overrides the env var) |
| `--seed <yaml>` | seed a **new** database from YAML instead of the embedded seed |

## Settings UI

Tray → **Settings…**. Everything saves automatically to SQLite; the agent
watches the revision counter and reloads within ~600 ms.

- **Contexts** list, default-context star, add context. Each row shows `layers · rules`.
- **Inside a context**: a one-line **Matches** summary + rule count.
  **Edit matches…** opens a modal for process/title rules; it scrolls
  independently, so a long rule list cannot push the pad deck out of the page.
- **Layer tabs** with the default layer starred, add an existing layer or a new
  one, reorder with `‹ ›`, remove from the context.
- **Used in**: toggle which contexts share the current layer.
- **Pad deck**: three physical keycaps plus the knob. Under each key sit
  `TAP` / `2×` / `HOLD` cards. The knob is spatial: **CCW card on the left, knob
  in the middle, CW card on the right**, with press `TAP`/`2×`/`HOLD` cards
  underneath. Clicking a keycap or a card opens the action editor. The deck uses
  the layer colour for its glow and gesture chips.
- **Action types**: key/chord, key sequence, type text, media key, scroll,
  layer action, peek legend, open app/file, URL, run command, window action,
  nothing.
- **Layer actions**: switch, next key only, while held, next, previous, context
  default.
- **Settings**: OSD on/off, show on context change, show on press, duration,
  double-tap/hold thresholds, knob acceleration and multipliers, read-only pad
  mapping.
- On a press the OSD becomes a **physical-pad HUD**: the keycap (or knob
  segment) depresses and ripples, and an action toast shows what ran
  (`→ Previous`), so media keys are visible even with no player.
- Each keycap shows the **tap** as its main label and `2×` / `H` secondary lines
  for double and hold; the knob-press row gets the same `2×` / `H` line. A press
  toast always names the gesture that actually fired.
- On a layer change the active rail pill and the big layer name animate with a
  light sweep.
- The HUD **does not** appear on app/context switches or on config saves. It
  shows on presses, layer changes, peek and explicit preview.
- **Settings → Legend monitor** picks `primary monitor` (default, fixed
  position) or `focused window`. **Preview legend** shows the current layer
  without touching the pad.
- **Export** writes `%APPDATA%\macrokey\export.yaml` (backup, not the runtime
  source of truth).
- **Reset to factory defaults** reseeds from the embedded seed.
- While Settings is foreground, pad actions are paused and transient gestures
  are cleared. **Preview legend** and the tray legend still work; return focus
  to the target app to resume the pad.
- For a HUD preview without the agent, open `agent/ui/index.html?demo=1` in a
  browser.

## Taskbar status

The tray icon is the taskbar surface for a tray-only agent:

- **Icon colour** = the active layer's colour.
- **Tooltip** = `macrokey · <context> · <layer>`, plus the last action after a
  press (`macrokey · VS Code · Debug · Step over`).
- Updates on context changes, layer changes and presses; the tooltip is capped
  at 120 chars.
- Right-click still gives **Settings…**, **Show current layer** and **Quit**.

Windows has no persistent text label next to a tray icon; hover is the native
way to read it. The colour answers "which layer" at a glance.

## Database

SQLite, WAL, foreign keys. Tables:

```
contexts(id, label, default_layer, position)
layers(id, label, color, position)
context_layers(context_id, layer_id, position)
bindings(layer_id, input, gesture, action)   -- action is JSON
rules(position, process, title, class, context_id)
settings(id=1, json)                         -- pad, gestures, knob, OSD, default_context
meta(key, value)                             -- schema, revision
```

The UI sends the whole state and the agent validates it (unknown keys, layer
references, rule contexts) before writing a transaction. A bad save never
reaches the database.

## Test without touching your config

Seed a throwaway database where every press appends to `%TEMP%\macrokey-test.log`
instead of typing into apps. Stop the running tray agent first; the
single-instance guard would otherwise forward the second launch to it.

```powershell
$env:MACROKEY_DB = "$env:TEMP\macrokey-test.db"
Remove-Item "$env:MACROKEY_DB*" -ErrorAction SilentlyContinue
.\target\debug\macrokey-agent.exe --seed ../../config/agent.test.yaml
```

## Dev tools

| Tool | Use |
|---|---|
| `tools/db_dump.py` | print contexts, layers, membership, rules, bindings, revision |
| `tools/db_bump.py` | change a label and bump the revision (live-reload test) |
| `tools/cdp_eval.mjs` | evaluate JS in the settings WebView over CDP |
| `tools/hid_log.py` / `rawinput_log.py` / `decode_capture.py` | pad diagnostics |

For the CDP helper, run the app with
`WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=9222`.

## Troubleshooting

- **No tray icon:** Windows hides new tray icons in the overflow. Open the tray
  overflow and drag the macrokey ring onto the taskbar. If Settings is not
  visible, launch the app again; the second launch focuses the running instance.
- **Settings does not open on an existing install:** right-click the tray icon,
  or start with `MACROKEY_SETTINGS=1`.
- **Pad does nothing:** confirm the firmware with
  `python tools/rawinput_log.py -s 60`. You should see `F13`–`F18` from the pad's
  device path. Raw Input is filtered by VID/PID, so another keyboard cannot
  trigger the agent.
- **`cargo run` exits immediately:** a release/tray instance is already running.
  Stop it from the tray (**Quit**) or `Stop-Process -Name macrokey-agent` first;
  the single-instance guard forwards the second launch to the running agent.
- **Pad does nothing while Settings is open:** intentional. Settings is a
  foreground window; use **Preview legend** or return focus to the target app.
- **An action does nothing in one app:** that app may be elevated. Windows blocks
  synthetic input from a non-elevated process (UIPI); the failure is recorded in
  `agent.log`.
- **Diagnostics:** `%APPDATA%\macrokey\agent.log` (or `agent.log` next to a
  custom `--db` file). It rotates to `agent.log.1` at 2 MB. Include the tail in
  bug reports.

## Not yet

- Import from YAML (export exists).
- Suppression of `F13`–`F18` (they still reach the focused app; harmless in most
  apps).
- Installer, code signing, opt-in autostart, auto-update.
- Schema migrations and automated engine/UI tests.
- Per-game Steam contexts (the client is `steamwebhelper.exe`; games are their
  own process).

## Layout

```
ui/index.html              OSD (context · layer, key/knob chips); ?demo=1 preview
ui/settings.html           context/layer/gesture editor, rules, settings
src/db.rs                  SQLite schema, seed, load/save, revision, export
src/config.rs              in-memory model + validation, action types
src/engine.rs              context/layer resolution, gestures, dispatch
src/context.rs             foreground watcher (150 ms poll)
src/input.rs               Raw Input thread on a hidden window
src/actions.rs             SendInput / ShellExecute / window actions
src/osd.rs                 window flags, placement, show/hide
src/tray.rs                dynamic tray icon colour and status text
src/logging.rs             append-only agent.log
src/commands.rs            Tauri IPC (load/save/reset/export/current app)
src/main.rs                Tauri setup, first-run Settings, single instance, tray
```
