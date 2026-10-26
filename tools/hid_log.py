"""Capture what the macro pad actually emits, decoded, from every HID collection.

Usage:
    python tools/hid_log.py                 # 30 s
    python tools/hid_log.py -s 120
    python tools/hid_log.py --until logs/stop

Writes a log to logs/hid-<timestamp>.log and prints a live summary.

The pad still types into the focused window, so put the caret somewhere harmless.
"""

import argparse
import datetime
import os
import sys
import threading
import time

import hid

VID, PID = 0x1189, 0x8890

KEYBOARD_MODIFIERS = ["LCtrl", "LShift", "LAlt", "LMeta", "RCtrl", "RShift", "RAlt", "RMeta"]

USAGE_NAMES = {
    0x04: "A", 0x05: "B", 0x06: "C", 0x07: "D", 0x08: "E", 0x09: "F", 0x0A: "G",
    0x0B: "H", 0x0C: "I", 0x0D: "J", 0x0E: "K", 0x0F: "L", 0x10: "M", 0x11: "N",
    0x12: "O", 0x13: "P", 0x14: "Q", 0x15: "R", 0x16: "S", 0x17: "T", 0x18: "U",
    0x19: "V", 0x1A: "W", 0x1B: "X", 0x1C: "Y", 0x1D: "Z",
    0x1E: "1", 0x1F: "2", 0x20: "3", 0x21: "4", 0x22: "5", 0x23: "6", 0x24: "7",
    0x25: "8", 0x26: "9", 0x27: "0",
    0x28: "Enter", 0x29: "Esc", 0x2A: "Backspace", 0x2B: "Tab", 0x2C: "Space",
    0x2D: "Minus", 0x2E: "Equal", 0x2F: "LeftBracket", 0x30: "RightBracket",
    0x31: "Backslash", 0x33: "Semicolon", 0x34: "Quote", 0x35: "Grave",
    0x36: "Comma", 0x37: "Dot", 0x38: "Slash", 0x39: "CapsLock",
    0x3A: "F1", 0x3B: "F2", 0x3C: "F3", 0x3D: "F4", 0x3E: "F5", 0x3F: "F6",
    0x40: "F7", 0x41: "F8", 0x42: "F9", 0x43: "F10", 0x44: "F11", 0x45: "F12",
    0x46: "PrintScreen", 0x47: "ScrollLock", 0x48: "Pause", 0x49: "Insert",
    0x4A: "Home", 0x4B: "PageUp", 0x4C: "Delete", 0x4D: "End", 0x4E: "PageDown",
    0x4F: "Right", 0x50: "Left", 0x51: "Down", 0x52: "Up", 0x53: "NumLock",
    0x54: "NumSlash", 0x55: "NumAsterisk", 0x56: "NumMinus", 0x57: "NumPlus",
    0x58: "NumEnter", 0x59: "Num1", 0x5A: "Num2", 0x5B: "Num3", 0x5C: "Num4",
    0x5D: "Num5", 0x5E: "Num6", 0x5F: "Num7", 0x60: "Num8", 0x61: "Num9",
    0x62: "Num0", 0x63: "NumDot", 0x64: "NonUSBackslash", 0x65: "Application",
    0x66: "Power", 0x67: "NumEqual", 0x68: "F13", 0x69: "F14", 0x6A: "F15",
    0x6B: "F16", 0x6C: "F17", 0x6D: "F18", 0x6E: "F19", 0x6F: "F20", 0x70: "F21",
    0x71: "F22", 0x72: "F23", 0x73: "F24",
    0xE0: "LCtrl", 0xE1: "LShift", 0xE2: "LAlt", 0xE3: "LMeta",
    0xE4: "RCtrl", 0xE5: "RShift", 0xE6: "RAlt", 0xE7: "RMeta",
}

CONSUMER_NAMES = {
    0x00B5: "NextTrack", 0x00B6: "PrevTrack", 0x00CD: "PlayPause", 0x00E2: "Mute",
    0x00E9: "VolumeUp", 0x00EA: "VolumeDown", 0x0182: "Favorites",
    0x0192: "Calculator", 0x019E: "ScreenLock", 0x0221: "WebSearch",
    0x0223: "WebHome", 0x0224: "WebBack", 0x0225: "WebForward",
    0x006F: "BrightnessUp", 0x0070: "BrightnessDown",
}


def name_usage(code):
    return USAGE_NAMES.get(code, f"0x{code:02X}")


def interface_label(info):
    usage = (info["usage_page"] << 16) | info["usage"]
    known = {
        (1, 6): "KEYBOARD",
        (1, 2): "MOUSE",
        (12, 1): "CONSUMER",
        (0xFF00, 1): "VENDOR",
    }
    kind = known.get((info["usage_page"], info["usage"]), "OTHER")
    return f"MI{info['interface_number']}/{kind} up=0x{info['usage_page']:02X} usage=0x{info['usage']:02X} (0x{usage:08X})"


class Logger:
    def __init__(self, out, lock):
        self.out = out
        self.lock = lock

    def write(self, line):
        stamp = datetime.datetime.now().strftime("%H:%M:%S.%f")[:-3]
        text = f"{stamp} {line}"
        with self.lock:
            print(text, flush=True)
            self.out.write(text + "\n")
            self.out.flush()


def watch(info, stop, log, t0):
    label = interface_label(info)
    dev = hid.device()
    try:
        dev.open_path(info["path"])
    except OSError as exc:
        log.write(f"[{label}] open failed: {exc}")
        return
    dev.set_nonblocking(1)
    log.write(f"[{label}] watching")

    prev_keys = set()
    prev_mods = set()
    last_consumer = None
    last_buttons = 0
    prev_report = None

    while not stop.is_set():
        try:
            data = dev.read(64, 50)
        except OSError as exc:
            log.write(f"[{label}] read failed: {exc}")
            break
        if not data:
            continue

        raw = bytes(data)
        if raw == prev_report:
            continue
        prev_report = raw
        age = time.perf_counter() - t0

        if info["usage_page"] == 1 and info["usage"] == 6:
            # Windows hidapi includes the report id as byte 0.
            body = raw[1:] if len(raw) > 8 else raw
            mods = {KEYBOARD_MODIFIERS[i] for i in range(8) if body[0] & (1 << i)} if body else set()
            keys = {b for b in body[2:8] if b} if len(body) >= 8 else set()
            downs = keys - prev_keys
            ups = prev_keys - keys
            mods_down = mods - prev_mods
            mods_up = prev_mods - mods
            if downs or ups or mods_down or mods_up:
                parts = []
                if mods_down:
                    parts.append("".join(f"+{m}" for m in sorted(mods_down)))
                if downs:
                    parts.append(" ".join(f"DOWN({name_usage(k)})" for k in sorted(downs)))
                if ups:
                    parts.append(" ".join(f"up({name_usage(k)})" for k in sorted(ups)))
                if mods_up:
                    parts.append("".join(f"-{m}" for m in sorted(mods_up)))
                log.write(f"[{label}] t={age:7.3f}s {' '.join(parts)}   raw={raw.hex(' ')}")
            prev_keys, prev_mods = keys, mods

        elif info["usage_page"] == 12 and info["usage"] == 1:
            body = raw[1:] if len(raw) >= 2 else raw
            usage = int.from_bytes(body[:2], "little") if len(body) >= 2 else 0
            if usage != last_consumer:
                what = CONSUMER_NAMES.get(usage, f"0x{usage:04X}")
                state = "DOWN" if usage else "up"
                log.write(f"[{label}] t={age:7.3f}s {what} {state}   raw={raw.hex(' ')}")
                last_consumer = usage

        elif info["usage_page"] == 1 and info["usage"] == 2:
            body = raw[1:] if len(raw) >= 4 else raw
            buttons = body[0] if body else 0
            dx = body[1] if len(body) > 1 else 0
            dy = body[2] if len(body) > 2 else 0
            wheel = body[3] if len(body) > 3 else 0
            dx = dx - 256 if dx > 127 else dx
            dy = dy - 256 if dy > 127 else dy
            wheel = wheel - 256 if wheel > 127 else wheel
            if buttons != last_buttons or dx or dy or wheel:
                log.write(
                    f"[{label}] t={age:7.3f}s buttons=0x{buttons:02X} dx={dx} dy={dy} wheel={wheel}"
                    f"   raw={raw.hex(' ')}"
                )
                last_buttons = buttons

        else:
            if any(raw):
                log.write(f"[{label}] t={age:7.3f}s raw={raw.hex(' ')}")

    dev.close()


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("-s", "--seconds", type=float, default=30)
    ap.add_argument("--until", default=None, help="stop when this file exists")
    ap.add_argument("--include-vendor", action="store_true",
                    help="also watch the 0xFF00 config interface (usually silent)")
    ap.add_argument("-o", "--out", default=None)
    args = ap.parse_args()

    infos = [
        d for d in hid.enumerate(VID, PID)
        if args.include_vendor or d["usage_page"] != 0xFF00
    ]
    if not infos:
        print("macro pad not found", file=sys.stderr)
        return 1

    os.makedirs("logs", exist_ok=True)
    out_path = args.out or os.path.join(
        "logs", datetime.datetime.now().strftime("hid-%Y%m%d-%H%M%S.log")
    )
    stop = threading.Event()
    lock = threading.Lock()

    with open(out_path, "w", encoding="utf-8") as out:
        log = Logger(out, lock)
        log.write(f"# capturing {len(infos)} collections; stop after {args.seconds:.0f}s"
                  + (f" or when {args.until} appears" if args.until else ""))
        t0 = time.perf_counter()
        threads = [threading.Thread(target=watch, args=(i, stop, log, t0), daemon=True) for i in infos]
        for t in threads:
            t.start()

        try:
            while time.perf_counter() - t0 < args.seconds:
                if args.until and os.path.exists(args.until):
                    log.write("# stop file seen")
                    break
                time.sleep(0.1)
        except KeyboardInterrupt:
            pass
        finally:
            stop.set()
            for t in threads:
                t.join(timeout=1.5)
            log.write("# done")

    print(f"\nsaved: {out_path}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
