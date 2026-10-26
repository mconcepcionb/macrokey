# Measurement log

Fill this in as experiments are run. Dates, machine, firmware state.

## 2026-10-25 — initial inspection (no writes)

- Device present as `USB\VID_1189&PID_8890\7&1569F8C9&0&2`, rev `0x0000`
- 4 HID interfaces; report lengths as documented in [device.md](device.md)
- `hidapi` read of `MI_00` and `MI_03` fails (`read error`)
- Raw Input works; `tools/rawinput_selftest.py` captures synthesized F13/F14

## E1 — current keymap and hold semantics ✅

Capture: `logs/e1-current-keymap.log`, decoded with
`python tools/decode_capture.py logs/e1-current-keymap.log`.

### Factory mapping (before any write)

| Physical input | Sends | Consumer usage |
|---|---|---|
| key 1 (left) | Previous track | `0xB6` |
| key 2 (middle) | Play / Pause | `0xCD` |
| key 3 (right) | Next track | `0xB5` |
| knob CCW | Volume down | `0xEA` |
| knob CW | Volume up | `0xE9` |
| knob press | Mute | `0xE2` |

So the pad was programmed as a media remote: 3 transport keys, volume knob,
mute on press.

### Delivery path

- All six arrived as **`RIM_TYPEHID`** (not `RIM_TYPEKEYBOARD`), from
  `\\?\HID#VID_1189&PID_8890&MI_02&Col02#...` — the consumer-control
  collection.
- HID report: 3 bytes, **report ID `0x02`**, two-byte consumer usage
  little-endian, release = `02 00 00`.
- Nothing came from `MI_00` or `MI_02&Col01`, because no binding was a plain
  keyboard key.
- Raw Input does not translate this path to a virtual key; the agent has to
  read the usage from the HID report. Handled in `tools/decode_capture.py`.

### Timing and gestures — the important part

| Observation | Value | Consequence |
|---|---|---|
| Tap press duration | 138–176 ms | informal tap threshold 250 ms is fine |
| Hold key 1 | **3.484 s down, then up** | **hold is reported** |
| Hold key 2 | **3.901 s down, then up** | **hold is reported** |
| Hold knob press | **1.903 s down, then up** | long-press works |
| Auto-repeat while held | **none** | no repeat storms to filter |
| Knob, one detent | one down + one up, 5–11 ms apart | one event per detent, no duplicates |

This is the best case: the firmware reports real key-down/key-up, so
**momentary layers, hold-to-modify and hold+turn are all viable**. No need for
tap-only fallbacks.

### Caveat for E3

This was measured on the **consumer** path. The keyboard path may behave
differently (a keyboard binding is a macro sequence in firmware, and cheap
firmware sometimes fires the whole sequence immediately). E3 must confirm that
`F13`–`F18` also report a held key rather than an instant press/release.

## E2 — knob dynamics ✅

One down/up pair per detent, 5–11 ms apart, no duplicates.

Measured detent intervals from E3:

| Spin | Interval between detents |
|---|---|
| slow (deliberate) | 680–990 ms |
| fast (as fast as possible) | **38–120 ms**, fastest 38 ms |

So max sustainable rate is roughly 25 detents/s. Acceleration buckets:
`≥120 ms` = 1×, `60–120` = 2×, `30–60` = 4×, `<30` = 8×. Debounce can be as
low as 8–10 ms.

## E3 — phantom keys ✅

Flashed `config/pad-single-layer.yaml` (`upload` exit 0) and captured again:
`logs/e3-phantom.log`.

| Input | VKey | Scan code | Delivery |
|---|---|---|---|
| key 1 (left) | `F13` | `0x64` | `RIM_TYPEKEYBOARD`, **`MI_00`** |
| key 2 (middle) | `F14` | `0x65` | `MI_00` |
| key 3 (right) | `F15` | `0x66` | `MI_00` |
| knob press | `F16` | `0x67` | `MI_00` |
| knob CCW | `F17` | `0x68` | `MI_00` |
| knob CW | `F18` | `0x69` | `MI_00` |

The flash works, the order matches the physical layout, and all six inputs are
distinct. Nothing arrived on `MI_02` any more.

### Hold behaviour on the keyboard path

Different from the consumer path, and worth knowing:

- The firmware **holds the key down** until release (one long press, not a
  synthetic tap).
- Windows then applies **auto-repeat**: after ~500 ms, a repeated `DOWN` every
  ~30 ms until `UP`.

Example (key 1 held ~2 s):

```
157.074 DOWN F13
157.591 DOWN F13   <- +517 ms, first repeat
157.624 DOWN F13   <- +33 ms ...
...  ~60 repeats ...
159.069 UP   F13
```

Consequences for the agent:

1. Hold-to-modify works on both paths.
2. Repeated `DOWN`s are **repeats, not presses** — the engine must dedupe by
   maintaining a pressed-set and only treating a `DOWN` as new when the input is
   not already down.
3. Holding a key spams the focused app with `F13`–`F18`. Harmless, but it is
   another reason to keep the pad on phantom keys.
4. A tap is `DOWN` → `UP` < 250 ms, ignoring repeats. Measured taps were
   109–208 ms.

## E4 — LED modes

_pending_ (cosmetic)

## E8 — leakage

_pending_ (needs a day of normal use)

## E9 — focus watcher

_pending_ (after M2)

## M1–M3 — agent vertical slice

- `cargo build` clean; Raw Input thread, layer rules, SendInput dispatch and OSD
  all start without errors.
- OSD verified visually on the secondary monitor with `MACROKEY_DEMO=1`
  (`logs/osd-crop.png`): two-row pill, labels fit, click-through.
- Real pad input confirmed: knob turns and all six inputs; the test layer wrote
  `k1 k2 k2 k3 k3 ccw×3 cw×3 press`, double-tap switched layer, hold showed the
  peek legend.

## M4 — context/layer model + SQLite

- Model corrected: a **context** owns an ordered list of **layers**; layers are
  a shared library across contexts. `Next`/`Prev` cycle inside the context.
- State moved from YAML to SQLite (`%APPDATA%\macrokey\macrokey.db`), WAL +
  foreign keys. Tables: contexts, layers, context_layers, bindings (action JSON),
  rules, settings JSON, meta (schema, revision).
- Settings window: contexts, layer tabs, per-input tap/double/hold, action editor,
  app rules, used-in-context checkboxes, settings panel. Auto-save on change.
- Live reload verified: editing the DB externally bumped `revision`; the engine
  logged `config reloaded` within ~600 ms.
- Auto-save verified through CDP: `addContext()` created a context + layer and
  persisted it; `setGesture('k2','tap',{...mute})` updated the binding row;
  `reset_config` restored the five default contexts.
- Fixed: `serde_json` was sorting object keys (contexts/layers) on every save;
  enabled `preserve_order` so the editor and `Next`/`Prev` order are stable.
- Persistence round-trip covered by `cargo test` (`sqlite_round_trip`).

### Still to verify with the pad

The new gesture engine executes taps on **release**, holds at 420 ms, and uses
the 260 ms double-tap window. It has not been exercised on hardware since the
refactor; the next press session should confirm tap/double/hold on a key and on
the knob press, plus a `switch` / `oneshot` / `momentary` layer transition.

## First physical test of the gesture engine

Log from the running agent:

```
[engine] K1 -> Previous [General / Media]      (10 taps)
[engine] layer -> nav (context general)        (double-tap worked)
[engine] context windowsterminal.exe | OC ... -> opencode
```

So input, tap, and double-tap all fired. It *looked* like nothing happened
because the action was a media key with no player running, and the OSD only
showed the layer, not the action. Fixed: the OSD now prints the fired action
(`General · Media  Previous`) and the settings editor puts each context's rules
inside that context instead of a global panel.

The context reset to `opencode` when focus moved to the terminal — by design:
changing context starts at that context's default layer.

## HUD

- Rebuilt as a physical-pad HUD: context + layer rail, hero layer name, three
  keycaps, knob with three labels, action toast.
- Press mode: keycap depresses, glows and ripples; action toast shows the fired
  action; no full-card entrance on rapid presses.
- Layer mode: active rail pill pops and the hero name swaps with a light sweep.
- Verified with real knob turns (`→ Vol +`), layer changes (`layer -> nav`),
  arrow actions in Nav, and wheel acceleration in opencode.
- `osd.monitor` works (`focused` / `primary`); `Preview legend` in Settings draws
  the selected layer without touching the pad.
- Noise fix: the HUD no longer appears on config reloads (settings auto-save
  bumps the revision often) or on app/context changes. Default monitor is
  `primary`, so the position is stable; a context change hides a lingering HUD.
- Taskbar status: the tray icon colour follows the layer colour and the tooltip
  is `macrokey · context · layer` (plus the action on press). Verified in the
  log: `[tray] macrokey · opencode · Chat (#c4b5fd)` and
  `[tray] macrokey · General · Media (#7dd3fc)`.
- Config UI: replaced the gesture table with a pad deck — three keycaps + knob,
  each with TAP/2×/HOLD cards underneath, layer-coloured glow, keycap hover and
  press states. The knob cluster is spatial: CCW card left, knob centre, CW card
  right, press gestures below. Clicking a cap or card opens the action editor;
  verified `General · Media · Key 1 / Tap / media`.
- Rules moved from the context page into a **Matches modal**. The page shows a
  one-line summary + count + `Edit matches…`; the modal scrolls independently,
  so long rule lists cannot push the deck out. Verified open/add/summary;
  restored the Steam rule afterwards (DB back to the original 5 rules, in order).
- OSD gestures: the payload now carries `gestures` per input; keycaps show the
  tap as the main label plus `2×` / `H` secondary lines, and the knob press row
  shows `2× · H`. Verified with the demo (`2× Save all`, `H Nav (held)`,
  `2× Next layer · H Peek legend`).
