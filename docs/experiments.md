# Experiments

Each experiment is cheap and answers one question. Record results in
[results.md](results.md) so the design decisions stay grounded.

Before any of this, put the caret somewhere harmless (Notepad), because the pad
types into the focused window.

---

## E1 — What does the pad currently send? (hold, repeat, which interface)

**Answers:** which collection emits keys, whether holds are reported, whether
the firmware auto-repeats, current keymap.

```powershell
python tools/rawinput_log.py -s 300 --until logs/stop
```

Then, with the log running:

1. tap key 1 (left), pause 1 s
2. tap key 2, pause
3. tap key 3, pause
4. **hold key 1 for 3 s**, release
5. **hold key 2 for 3 s**, release
6. turn the knob 5 detents counter-clockwise, pause
7. turn it 5 detents clockwise, pause
8. press the knob once, pause, press twice quickly
9. hold the knob press for 2 s, release

Stop with `New-Item logs/stop`.

Read the log:

- `dev=` should be the pad's path (`\\?\HID#VID_1189&PID_8890&MI_00...` or
  `...MI_02...`). Note which one.
- `DOWN` → `UP` spacing tells us hold semantics. If every key produces
  `DOWN`+`UP` instantly even when held, the firmware is one-shot.
- Repeated `DOWN` events without `UP` = firmware auto-repeat. Note the rate.
- Knob: count events per detent. If a detent produces two events, we decimate.

**Pass:** we can classify each input as `hold-capable` or `one-shot`.

---

## E2 — Knob dynamics

**Answers:** detents per revolution, max event rate, debounce needed.

```powershell
python tools/rawinput_log.py -s 60
```

Spin the knob as fast as possible for 5 s each direction, then slowly (1 detent
per second). Compute from the log:

- events per detent (fast vs slow)
- minimum interval between events
- whether direction can be inferred from the key itself (F17/F18) — it can, so
  no need for delta accumulation

**Pass:** a multiplier table for acceleration (e.g. 1× below 40 ms, 2× below
20 ms, 4× below 10 ms).

---

## E3 — Flash the phantom keys and verify

**Answers:** whether the firmware accepts `F13`–`F18`, and which interface the
keys come from.

```powershell
ch57x-keyboard-tool validate config/pad-single-layer.yaml
ch57x-keyboard-tool upload   config/pad-single-layer.yaml
python tools/rawinput_log.py -s 120
```

Press each key and the knob. Expected, in order:

```
key 1        DOWN vk=F13   UP vk=F13
key 2        DOWN vk=F14   UP vk=F14
key 3        DOWN vk=F15   UP vk=F15
knob CCW     DOWN vk=F17   UP vk=F17
knob press   DOWN vk=F16   UP vk=F16
knob CW      DOWN vk=F18   UP vk=F18
```

If `F13`–`F15` never arrive, the report descriptor caps the key array: fall back
to `ctrl-alt-shift-j/k/l` (one chord per input) or to consumer usages. Record
which.

Also check `scan=` values: a firmware-synthesized key usually has
`scan=0x00`, which is a useful extra fingerprint.

**Pass:** all six inputs arrive as distinct VKs from the pad's device path.

---

## E4 — LED modes (optional, cosmetic)

```powershell
ch57x-keyboard-tool led 0     # off
ch57x-keyboard-tool led 1     # likely steady
ch57x-keyboard-tool led 2
...
ch57x-keyboard-tool led 8
```

Note what each mode looks like. Keep one static mode; **do not** use LEDs as the
layer indicator (they can't show which layer is active, and changing them on
every focus change is slow and can glitch).

---

## E5 — Does the vendor interface emit key events?

**Answers:** whether we could read presses without the OS (nice, not required).

```powershell
python tools/hid_log.py -s 60 --include-vendor
```

Press keys and turn the knob. If `MI_01` stays silent, close the question.
(The `--include-vendor` flag adds the `0xFF00` interface; all-zero reports are
skipped so the log stays readable.)

---

## E6 — Host-side layer select (`0xA1`) — optional, low value

Only if we ever want firmware-side layers as a fallback. Requires writing layer
2 with different keys, then sending `0xA1 0` / `0xA1 1` and observing.

Risk: low (it is a documented "read/select" style command in one
implementation), but it is unverified for this dialect and gives us nothing the
host engine cannot do. Skip unless curious.

---

## E7 — Driverless write path (optional, nice-to-have)

`ch57x-keyboard-tool` needs USBDK. `VMacropad` writes through plain `hidapi` to
`MI_01` using report ID `0x03` and a 65-byte output report. If that works here,
we can ship a configurator with no driver install.

Do this only after E1–E3, and only with a config we can re-flash:

1. build the exact LED message (`03 B0 18 01` padded to 64)
2. `hid.write([0x03] + payload_64)`
3. see if the LED changes

If it works, extend to key records and compare against the tool's output.

---

## E8 — Phantom-key leakage

Use the machine normally for a day with the pad flashed to `F13`–`F18`.

Watch for: games binding F13, OBS/streaming decks, IDEs, browsers, remote-desktop
tools, accessibility software. If something reacts, note it. Only then decide
whether to add suppression (LL hook or Interception driver).

---

## E9 — Focus watcher edge cases

While the M2 agent is running, check that the layer follows focus in:

- a normal Win32 app (Notepad, Explorer)
- an elevated app (Task Manager, an admin terminal)
- a UWP/Store app
- a browser with several tabs (title changes without focus change)
- a fullscreen game / borderless game
- RDP or a VM window
- two monitors with different DPI

**Pass:** the layer is correct within ~200 ms, and never gets stuck.

---

## Results template

Copy into [results.md](results.md):

```
E1  device path:        MI_00 / MI_02
    hold:               reported / one-shot
    repeat:             none / ~__ Hz
    knob events/detent: __
    current keymap:     key1=__ key2=__ key3=__ knob=__/__/__

E2  min interval:       __ ms
    fast rate:          __ events/s
    acceleration:       1× <__ms, 2× <__ms, 4× <__ms

E3  F13..F18:           all ok / fallback:__
    scan codes:         __

E4  LED modes:          __

E8  leakage:            __
```
