# Device and firmware reference

Everything below is either **measured on this unit**, **read from the
vendor-style protocol**, or **inferred from other units**. The confidence is
marked so we don't build on a guess.

---

## 1. Physical device

Typical AliExpress "3-key + 1-knob" macro pad:

- 3 mechanical switches in a row, hot-swap sockets
- 1 rotary encoder with a push switch
- acrylic/PCB sandwich case
- RGB LEDs under the keys, diffused by the plate
- one USB-C port (A-to-C cable; many of these boards omit the CC pull-downs, so
  a C-to-C cable may not enumerate at all)

The microcontroller is **not identifiable from software**. "CH57x" is the name
the ecosystem settled on; the actual die may be a CH552, CH573, CH579 or
something else. It does not matter for our plan: the protocol belongs to the
firmware, and the firmware is identified by the USB IDs and interface layout.

> If we ever want custom firmware, the case has to be opened and the MCU marking
> read. See §8.

---

## 2. USB topology (measured)

```
Composite device  VID 0x1189 (Trisat Industrial)  PID 0x8890  bcdDevice 0x0000
  no manufacturer / product / serial strings
  4 HID interfaces:

  MI_00  usage 0x0001:0x0006  keyboard   in  9 B   out  2 B
  MI_01  usage 0xFF00:0x0001  vendor     in 65 B   out 65 B   <- configuration
  MI_02  usage 0x0001:0x0006  keyboard   in 12 B
         usage 0x000C:0x0001  consumer   in  3 B
  MI_03  usage 0x0001:0x0002  mouse      in  5 B
```

Notes:

- Windows report lengths include the report ID byte. `MI_01` therefore carries a
  **64-byte payload** with report ID `0x03` (or, seen from libusb, 64 raw bytes
  whose first byte is `0x03`).
- `MI_00` is a 6KRO keyboard report (`modifier`, reserved, 6 key slots).
- `MI_02` carries two collections: another keyboard, and a **consumer control**
  (report ID `0x02`, two-byte usage, 3-byte reports). Media bindings are emitted
  here, not on the keyboard collection — verified in E1.
- The duplicated keyboard collection on `MI_02` is presumably the media/HID
  path; the consumer collection next to it carries media usages.
- The config endpoint used by `ch57x-keyboard-tool` and `macroknob` is
  **interface 1, endpoint `0x02` OUT**.
- Windows binds `usbccgp` → `HidUsb` → `kbdhid` / `mouhid`. That is why generic
  HID reads on the keyboard and mouse collections fail — the class drivers own
  them.

### Reading the protocol descriptor is not necessary

Windows exposes the report *lengths* via `HidP_GetCaps` (used by
`tools/probe_hid.py`). The raw report descriptor is not exposed through the
usual IOCTL on this stack; every attempt returned unrelated bytes. The lengths,
usage pages and observed behaviour are enough.

---

## 3. Firmware dialect: `ch57x-2`

`ch57x-keyboard-tool` maps `1189:8890` to model `ch57x-2` and implements it in
[`src/keyboard/k8890.rs`](../vendor/ch57x-keyboard-tool/src/keyboard/k8890.rs).
This is the implementation that **works on this unit**.

### Message format

64-byte reports, first byte `0x03`, everything zero-padded.

```
bind start     03 FE <layer+1> 01 01 00 00 00 00
one press      03 <slot> <(layer+1)<<4 | 01> <count> <index> <modifier> <hid_usage> 00 00
media          03 <slot> <(layer+1)<<4 | 02> <usage_lo> <usage_hi> 00 00 00 00
mouse          03 <slot> <(layer+1)<<4 | 03> <buttons> <dx> <dy> <wheel> <modifier> 00
bind finish    03 AA AA 00 ...
```

- A keyboard binding is sent as `count + 1` messages: one synthetic empty press
  `(0, 0)` followed by the real presses. The original author notes only
  *"for whatever reason empty key is added before others"*.
- Maximum **5 presses** per binding.
- **No inter-step delay** on this model; `{delay(...)}` is rejected by the tool.
- Slots: keys `1..12` (`0x01..0x0C`), then knob actions `13/14/15` =
  `ccw / press / cw` (`0x0D/0x0E/0x0F`).
- Our 3-key unit uses slots `1, 2, 3` and `13, 14, 15`.

### Layers

The wire format has a 4-bit layer number (`(layer+1) << 4`), and the tool
accepts up to 15. The vendor software and every report from other owners
describe **3 layers**. There is no software command in the vendor tooling to
change the active layer; on multi-key models it is a physical button on the case
edge, with LED indicators.

Our 3-key unit has **no such button**. So layer 1 is the only reachable layer.

`VMacropad` implements `0xA1 <layer>` as "select the active layer" but never
calls it, and `macropad-rebind` explicitly lists `0xA1` as *unverified* for this
dialect. There is one supporting hint: the official tool's LED sequence starts
with `03 A1 01`, i.e. literally "select layer 1" before writing a per-layer LED
mode. That raises the odds that `0xA1` is a real layer selector, but neither the
effect on the active *keymap* nor the exact semantics has been verified. Treat
host-side layer switching as **unproven and unnecessary**.

### LED

Two encodings exist in the wild:

```
ch57x-keyboard-tool   03 A1 01 ...      then  03 B0 18 <mode> ...   then  03 AA A1 ...
VMacropad             03 B0 08 <mode>
```

Both are one byte of "mode"; there is no colour or per-key addressing exposed
for this model. `ch57x-keyboard-tool led 1` is the supported call for `0x8890`.

The hardware can clearly address LEDs individually (press effects light one
key), but no host-reachable per-key colour has been found for this firmware
generation. **Do not use LEDs as a layer indicator**; use the OSD.

### Known limits and hazards

| Item | Value / behaviour |
|---|---|
| Bindings per control | 1 macro, up to 5 key presses (`ch57x-2`) |
| Layers stored | 3 in practice |
| Read-back | **none known** for `ch57x-2` |
| Modifiers | on 1-row/1-knob models, only on the **first** press of a sequence |
| Media keys | consumer usages, separate report path |
| Mouse actions | click / move / drag / wheel, 8-bit deltas |
| `0xFC` | **sets the hardware variant**; wrong values make keys appear dead. Never send casually |
| `0xEF` | **enters the WCH ISP bootloader**. No firmware image exists to restore. Never send |
| `0xAA 0xAA` | used as terminator; `VMacropad` treats it as "save to flash" |

There is no way to dump the current keymap. The YAML we flash is the source of
truth; if it drifts, re-flash.

---

## 4. What the OS sees

The pad is an ordinary HID keyboard. It has:

- no serial number (multiple identical pads cannot be told apart by USB alone
  until you look at the port path),
- no per-key identity,
- no vendor events on the keyboard collection.

Consequences:

- Any "layer" has to exist either in firmware (not reachable here) or in a host
  process.
- The host process must identify the pad by **device path / VID+PID+interface**,
  not by key codes alone.
- If the agent is not running, the pad still sends whatever the firmware holds.
- Raw Input delivers the two kinds of binding on different paths:

  | Binding | Raw Input type | Interface | Data |
  |---|---|---|---|
  | keyboard keys (`F13`…) | `RIM_TYPEKEYBOARD` | `MI_00` or `MI_02&Col01` | VKey + scan |
  | media keys | **`RIM_TYPEHID`** | `MI_02&Col02` | report ID `2` + 2-byte usage |

  A media binding is *not* translated into a virtual key for us; the report has
  to be parsed. `tools/decode_capture.py` does both.

Measured gesture behaviour (E1 consumer path, E3 keyboard path): key-down is
reported when pressed, key-up on release; on the keyboard path Windows adds
auto-repeat after ~500 ms (one `DOWN` every ~30 ms), so repeats must be filtered.
No auto-repeat on the consumer path. One event per knob detent (fastest 38 ms).
Holds of 1.9–3.9 s were confirmed.

---

## 5. Input capture options on Windows

| Method | Admin | Device identity | Can swallow | Observed |
|---|---|---|---|---|
| `hidapi` read of `MI_00` | no | yes | no | **fails** — `read error`, class driver owns it |
| `hidapi` read of `MI_01` | no | yes | n/a | opens; whether it emits events is untested |
| Raw Input (`RIDEV_INPUTSINK`) | no | **yes, device path** | no | **works**, verified with synthesized and (pending) real keys |
| Low-level keyboard hook | no* | no (timing only) | **yes** | works for normal apps; elevated apps bypass a non-elevated hook |
| Interception driver | install | yes (HWID filter) | **yes** | robust; used by Kanata; kernel driver |
| HidHide | install | n/a | **no** | explicitly cannot hide keyboards or block Raw Input |

\* An LL hook technically runs without admin, but it will not see input destined
for elevated windows unless the hook process is elevated too.

**Decision:** Raw Input for v1. It needs no driver, cannot lock the user out,
and gives us exactly the two things we need: per-device filtering and
key-down/key-up. If phantom-key leakage into apps becomes a problem, add an
optional LL hook (or Interception) later — the engine does not change.

---

## 6. What we still don't know (and how to find out)

These are the questions that shape the UX. They need the pad in hand; commands
are in [experiments.md](experiments.md).

1. ~~**Does the firmware report hold?**~~ **Yes** — measured 1.9–3.9 s holds
   with key-up on release, no auto-repeat (E1). Must be re-confirmed for the
   keyboard path in E3.
2. ~~**Does it auto-repeat?**~~ **No** on the consumer path (E1).
3. ~~**How many events per knob detent?**~~ **One** (E1). Fast-spin rate still
   unmeasured.
4. ~~**Is the knob press a clean down/up?**~~ **Yes** (E1).
5. ~~**Which collection emits the keys?**~~ Media bindings: `MI_02&Col02` as
   `RIM_TYPEHID`. Keyboard bindings: to be confirmed in E3.
6. ~~**Does the report descriptor actually accept `F13`–`F18`?**~~ **Yes** —
   flashed and verified: `F13`–`F18` arrive from `MI_00`, scan `0x64`–`0x69`,
   with OS auto-repeat while held (E3).
7. **Does `MI_01` emit anything on key presses?** If yes, we could read raw
   events without involving the OS at all.
8. **Does `0xA1 <layer>` change the live layer?** (Optional, low value.)

---

## 7. Prior art, and what to take from each

| Project | What it does | Take-away |
|---|---|---|
| `ch57x-keyboard-tool` | Rust writer, USBDK/libusb | The working `ch57x-2` protocol; keep using it to flash |
| `macropad-rebind` | WebHID writer/reader for `ch57x-1` | Best protocol documentation; shows a driverless HID path is possible |
| `hotkeyconfig` | F13–F24 combos + local listener on `k884x` | Validates the phantom-key idea; adds collision checks and autostart |
| `VMacropad` | Python, targets `0x8890`, context-aware profiles | Closest product to the goal; switches profiles by **rewriting the pad**, which we want to avoid |
| `macroknob` | Go, `0x8890` 6+1, CLI + web UI | Independent confirmation of slots and endpoint |
| `kanata` + `komokana` | App-aware layers for Windows | Proven UX model: context watcher sends layer changes to a mapping engine |
| QMK PR #26100 | CH579M port in progress | The only path to real firmware; high effort and risk |

---

## 8. Custom firmware: the honest assessment

Tempting, because it would give real layers, per-key RGB and a proper HID
descriptor. But:

- the MCU is unknown until the case is opened;
- there is **no read-back of program flash**, and no published image for this
  board, so a failed flash is unrecoverable;
- the only in-progress QMK support is for **CH579M** and is still a draft PR;
  our board may be a CH552 (no QMK, no USB bootloader in the same form);
- everything we want can be done host-side with zero risk.

**Recommendation:** do not touch the firmware. Revisit only if the host-side
design hits a wall that only firmware can solve (e.g. we need true hold
semantics and the firmware cannot provide them).

---

## 9. Quick reference: current firmware plan

`config/pad-single-layer.yaml`:

| Physical input | HID usage | Windows VK |
|---|---|---|
| key 1 (left) | `F13` (0x68) | `VK_F13` (0x7C) |
| key 2 (middle) | `F14` (0x69) | `VK_F14` (0x7D) |
| key 3 (right) | `F15` (0x6A) | `VK_F15` (0x7E) |
| knob CCW | `F17` (0x6C) | `VK_F17` (0x80) |
| knob press | `F16` (0x6B) | `VK_F16` (0x7F) |
| knob CW | `F18` (0x6D) | `VK_F18` (0x81) |

Deliberately **no modifiers** and **no multi-step macros**: the firmware stays
dumb and the host owns all semantics.
