# Architecture

## 1. The central constraint

The pad is a **closed-firmware HID keyboard**. It stores at most 3 keymaps, has
no readable configuration, no per-key identity, and the 3-key model has no
hardware layer switch. Everything contextual therefore has to happen on the
host.

That leaves four plausible designs.

| # | Design | How layers work | Verdict |
|---|---|---|---|
| A | Firmware layers | 3 keymaps in flash, switch with the case button | **Impossible** — our case has no button; only layer 1 is reachable |
| B | Rewrite the pad on focus change | Agent re-flashes the keymap whenever the app changes (what `VMacropad` does) | Works, but slow (~100s of ms), wear on flash, no read-back, fragile mid-write, and limited to the firmware's 3 keymaps and 5-press macros |
| C | **Dumb pad + smart host** | Agent listens for 6 unique phantom keys and resolves layers itself | **Chosen** — unlimited layers, real context, real OSD, zero firmware risk |
| D | Custom firmware | Replace the firmware | Deferred — unknown MCU, no firmware backup, QMK support still in progress |

C is also how `hotkeyconfig` (F13–F24) and `kanata`/`komokana` (app-aware layers)
work, which is reassuring.

---

## 2. Chosen design in one picture

```
   ┌──────────────┐   HID keyboard reports (F13…F18)
   │  macro pad   │ ─────────────────────────────────┐
   │  dumb layer  │                                  │
   └──────────────┘                                  ▼
                                          ┌────────────────────┐
                                          │  Raw Input thread   │  per-device filter
                                          │  (no driver)        │  down/up + timing
                                          └─────────┬──────────┘
                                                    │ InputEvent
                                                    ▼
   ┌────────────────┐   focus change    ┌────────────────────────┐
   │ focus watcher  │ ─────────────────▶│  layer resolver        │
   │ WinEventHook   │  process/title    │  manual > context >    │
   └────────────────┘                   │  default               │
                                        └─────────┬──────────────┘
                                                  │ Binding
                                                  ▼
                                        ┌────────────────────────┐
                                        │  action dispatcher     │──▶ SendInput / launch /
                                        │  + knob acceleration   │    URL / media / script
                                        └─────────┬──────────────┘
                                                  │ state change
                                                  ▼
                                        ┌────────────────────────┐
                                        │  OSD overlay window    │
                                        │  always-on-top, click- │
                                        │  through, fades out    │
                                        └────────────────────────┘
```

The pad keeps working (as a silent F13–F18 keyboard) if the agent dies. It
simply stops doing anything useful, and nothing needs to be restored.

---

## 3. Firmware contract

One layer. Six inputs, six phantom keys, no modifiers, no macros:

```
key 1 (left)   F13        knob CCW     F17
key 2 (middle) F14        knob press   F16
key 3 (right)  F15        knob CW      F18
```

Why phantom keys:

- `F13`–`F24` exist as HID usages but no physical keyboard produces them.
- They travel as ordinary keyboard reports, so no consumer-control grabbing,
  no volume OSD, no media-session fights.
- They are unique enough that a *global* listener can be dumb about modifiers.
- If we ever need more inputs, `F19`–`F24` are free.

Why not modifiers (`ctrl-alt-F13`):

- modifiers can leak into the focused app and trigger shortcuts;
- the firmware's 3x1 limitation only allows modifiers on the first press anyway;
- Raw Input already tells us which device sent the key, so uniqueness is not
  needed.

Verified assumption to check: that the firmware's report descriptor accepts
`0x68`–`0x6D`. See experiment **E3**.

---

## 4. Input layer

`RIDEV_INPUTSINK` raw input in a dedicated thread with a hidden window.

- Filter by **device path prefix** `\\?\HID#VID_1189&PID_8890` and the keyboard
  usage; do not filter by exact path, which changes with the USB port.
- Map `VKey` → logical input: `F13..F18` → `k1,k2,k3,knob_press,knob_ccw,knob_cw`.
- Track `DOWN`/`UP` per input; ignore synthetic repeats (`Flags & RI_KEY_BREAK`,
  plus a dedupe on repeated `DOWN` without `UP`).
- Timestamp every event for gesture detection and knob acceleration.
- Never inject our own events into the same listener: mark generated input with
  `dwExtraInfo = 0x4D414352` ("MACR") and drop it.

Gesture detection (timings measured in E1–E3, all configurable):

| Gesture | Rule | Use |
|---|---|---|
| tap | down→up < 250 ms (measured taps: 109–208 ms) | normal action |
| double-tap | two taps < 300 ms apart (measured: 273 ms) | next layer |
| triple-tap | three taps < 400 ms | previous layer (or "peek") |
| long-press | hold > 500 ms | peek legend / momentary modifier |
| hold + turn | key held while knob events arrive | alternate knob mode |
| chord | k1+k2 down within 40 ms | optional, reserved |

**Repeat handling is mandatory.** A held key emits one real `DOWN`, then — after
the Windows repeat delay (~500 ms) — a repeat `DOWN` every ~30 ms until `UP`.
Keep a pressed-set: a `DOWN` for an input already down is a repeat, ignore it for
gesture purposes (but keep it available for hold timing). Never treat repeats as
new presses, or a one-second hold becomes 18 "double taps".

Knob acceleration buckets from measured intervals (38–990 ms):

```
interval >= 120 ms -> 1x     60..120 ms -> 2x
30..60 ms          -> 4x     < 30 ms    -> 8x
```

If experiment **E1** shows the firmware does not report hold, long-press and
hold+turn are unavailable; double/triple taps still work, and the layer
modifier becomes a **latch** (tap k3 = latch shift until next action).

---

## 5. Context and layer model

A **context is not a layer**. A context is the environment: `General` (the
fallback when nothing matches), `VS Code`, `opencode`, `VLC`, `Steam`. It is
resolved from the focused process and window title.

A context **owns an ordered list of layers** plus one default layer:

```
General  -> media (default), nav
VS Code  -> edit (default), nav, debug, build
opencode -> chat (default), nav
VLC      -> playback
Steam    -> browse
```

Layers are a **shared library**. `nav` is one layer reused by General, VS Code
and opencode; editing it changes it in all three. This is deliberately not a
tree of copies.

### Resolution

1. focus rule → context (first match; fallback `general`)
2. inside that context:
   1. momentary layer (held)
   2. one-shot layer (next action)
   3. manual layer (switch / next / prev) if it belongs to the context
   4. the context's default layer

Rules of thumb:

- A focus change **to a different context** resets the manual/one-shot layer and
  starts at the new context's default. A title change **inside** the same
  context keeps the current layer.
- `next` / `prev` cycle only through the current context's layer list.
- `switch` is only valid for a layer in the current context. `momentary` and
  `oneshot` may target any layer, but the editor only offers the context's
  layers.
- A one-shot layer is consumed by the next action, then the previous layer
  returns.
- `back` returns to the context default.

### Layer actions

| Mode | Behaviour |
|---|---|
| `switch` | stay on the target layer until another switch or context change |
| `momentary` | target layer while the key is held (make it a `hold` binding) |
| `oneshot` | target layer for exactly the next action |
| `next` / `prev` | cycle inside the current context |
| `back` | context default |

### Gestures and bindings

Each input has up to three bindings: `tap`, `double`, `hold`. A tap is evaluated
on release; if a `double` binding exists the tap waits one double-tap window. A
`hold` fires at the hold threshold and suppresses the tap. Windows auto-repeat
while a key is held is deduplicated.

Action kinds: `hotkey`, `sequence` (chords/text/delays), `text`, `media`,
`wheel`, `launch`, `url`, `script`, `window`, `layer`, `peek`, `none`.

### Persistence: data, not code

The live state is SQLite at `%APPDATA%\macrokey\macrokey.db`:

```
contexts(id, label, default_layer, position)
layers(id, label, color, position)
context_layers(context_id, layer_id, position)
bindings(layer_id, input, gesture, action_json)
rules(position, process, title, class, context_id)
settings(id=1, json)         -- pad, gestures, knob, OSD, default_context
meta(key, value)             -- schema, revision
```

- The settings window edits the state and auto-saves. Rust validates the whole
  state (keys, layer references, rule contexts) before a transaction.
- Every write bumps `meta.revision`; the agent polls it every 600 ms and reloads.
- `config/agent.draft.yaml` is only the **seed** for a new database.
- YAML is an **export/backup** format, not the runtime source of truth.

Nothing about a particular context, layer, key or action is compiled into the
agent; the engine only interprets the generic data.

---

## 6. On-screen feedback (OSD)

A small, transient, click-through overlay is the right amount of feedback for a
3-key pad — persistent HUDs are distracting; on-device LEDs cannot show text.

```
   ┌──────────────────────────────────────────────────────┐
   │ ● VS CODE      [Edit][Nav][Debug][Build]     MANUAL   │
   │ LAYER                                                │
   │ Debug                                  → Step over    │
   │ ┌──────────┐ ┌──────────┐ ┌──────────┐   ╭────╮       │
   │ │ 1        │ │ 2        │ │ 3        │   │ ◉  │ ↺ Scroll│
   │ │ Step over│ │ Continue │ │Breakpoint│   ╰────╯ ⏺ Stop  │
   │ └──────────┘ └──────────┘ └──────────┘          ↻ Scroll│
   └──────────────────────────────────────────────────────┘
```

- 660 × 300 px card, bottom-centre of the chosen monitor, 48 px above the work
  area.
- **Top**: context dot + name, the context's layer rail (active pill in the layer
  colour), and a `MANUAL` / `ONCE` / `HELD` / `PREVIEW` badge.
- **Middle**: the layer name as the hero (gradient white → layer colour) and an
  action toast (`→ Previous`) on a press.
- **Bottom**: a physical representation of the pad — three keycaps with number,
  tap label and `2×`/`H` secondary lines for double/hold, and a knob with its
  three actions (`↺` / `⏺` / `↻`); the press row also carries a `2× · H` line.
- Animations: card entrance (blur + rise), light sweep on layer change, rail pill
  pop, hero name swap, keycap depress + glow + ripple, knob rotate/press, action
  toast slide. Press mode skips the full-card entrance so rapid presses do not
  flash.
- `osd.monitor` chooses `primary` (default, one fixed position) or `focused`;
  the editor has a **Preview legend** button that draws the selected layer
  without the pad.
- Show triggers: press, manual/one-shot/momentary layer change, peek, preview.
  **Not** app/context changes or settings saves — a context change hides any
  lingering HUD instead of popping a new one.
- Timing: 160 ms in, 1500 ms hold, 220 ms out. **Peek** keeps it until release.
- Window flags: `WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE |
  WS_EX_TRANSPARENT`; click-through; never steals focus; excluded from Alt-Tab.
- The window is 660×300 logical, so it scales cleanly on 100–200 % displays.

### Stack options

| Stack | OSD effort | Install size | Notes |
|---|---|---|---|
| **Tauri 2** (Rust + WebView2) | low (HTML/CSS) | ~10–20 MB | Best look/effort. Engine in Rust, OSD in CSS, tray + autostart plugins. WebView2 is already on Win11 |
| Electron | lowest | ~150 MB | Fine for a spike, heavy for a tray utility |
| C# WPF / WinUI | medium | ~5 MB self-contained | Native transparency and tray; needs the .NET SDK |
| Rust native (winit + egui, or raw Win32 + Direct2D) | high | ~3 MB | Smallest and fastest, most UI work |
| Python + PySide6 | low | ~80 MB packaged | Fastest to prototype; matches the existing probe scripts |

Recommendation: **Tauri 2**. The engine needs Rust anyway (raw input, WinEvent
hook, SendInput, layered windows), and CSS makes the OSD and a future settings
UI cheap. Fallback for a one-evening prototype: Python + PySide6.

---

## 7. Reliability rules

- **Never grab the pad exclusively.** If the agent crashes, the pad must not be
  dead. Raw Input gives us this for free.
- **Release everything on exit.** Track modifiers injected by us; send key-ups
  on shutdown, config reload and panic.
- **Autostart + single instance**; tray icon with pause/resume and "open
  config".
- **Startup self-check**: is the pad present? is the device path recognised?
  does a key produce the expected VK? show a tray warning if not.
- **Elevation**: `SendInput` into elevated windows is blocked by UIPI unless the
  agent runs elevated. Run unelevated by default; document the toggle.
- **Duplicate pads**: if two matching devices exist, prefer the one from the
  configured USB port path; otherwise show a chooser.
- **Focus watcher**: `SetWinEventHook(EVENT_SYSTEM_FOREGROUND)` plus a 500 ms
  polling fallback (some UWP transitions and RDP sessions miss the hook).
- **Tray status**: the icon colour is the active layer colour and the tooltip is
  `macrokey · context · layer` (plus the last action). It updates on context,
  layer and press events via the cached `update_tray` path.

---

## 8. Open questions that change the design

| # | Question | Result |
|---|---|---|
| 1 | Does a physical press emit `DOWN` … `UP` while held? | **YES** on both paths (E1, E3). Keyboard keys also auto-repeat through Windows (500 ms / 30 ms), so dedupe. |
| 2 | Does the firmware accept `F13`–`F18`? | **YES** — flashed and verified, `MI_00`, scan `0x64`–`0x69` (E3) |
| 3 | Is the knob one event per detent? | **YES** (E1), fastest interval 38 ms (E3) |
| 4 | Does `MI_01` emit key events? | untested, low value — Raw Input already works |
| 5 | Does `0xA1 <layer>` work? | untested, no longer needed |

---

## 9. Build order

| Milestone | Deliverable | Status |
|---|---|---|
| M0 | Experiments E1–E4 | **done** — hold, repeats, knob rate, F13–F18 verified |
| M1 | Rust input + dispatch skeleton | **done** — Raw Input, SendInput, tray, real pad verified |
| M2 | Context → layers + persistence | **done** — SQLite, rules, layer actions, auto-save, live reload |
| M3 | OSD | **done** — context · layer pill, peek, press highlight |
| M4 | Tray, autostart, packaging | tray done; autostart and installer pending |
| M5 | Settings UI | **done** — contexts, layers, gestures, rules, actions, settings |
| M6 | Import, suppression, per-game contexts | pending |

The investigation tools in `tools/` already cover M0; `rawinput_log.py` is the
same code path the M1 listener will use.
