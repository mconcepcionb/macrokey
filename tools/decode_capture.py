"""Decode a rawinput_log.py capture for the macro pad and print a clean timeline.

Handles both delivery paths seen on this device:
  * RIM_TYPEKEYBOARD  -> VK (after we flash F13-F18)
  * RIM_TYPEHID       -> consumer control (the factory media mapping)

Usage:
    python tools/decode_capture.py logs/e1-current-keymap.log
    python tools/decode_capture.py logs/e1-current-keymap.log --all
"""

import argparse
import re
import struct
import sys

PAD = "VID_1189&PID_8890"

CONSUMER = {
    0x00B5: "NextTrack", 0x00B6: "PrevTrack", 0x00CD: "PlayPause", 0x00E2: "Mute",
    0x00E9: "VolumeUp", 0x00EA: "VolumeDown", 0x0182: "Favorites",
    0x0192: "Calculator", 0x019E: "ScreenLock", 0x0221: "WebSearch",
    0x0223: "WebHome", 0x0224: "WebBack", 0x0225: "WebForward",
    0x006F: "BrightnessUp", 0x0070: "BrightnessDown",
}

VK_NAMES = {
    0x7C: "F13", 0x7D: "F14", 0x7E: "F15", 0x7F: "F16", 0x80: "F17",
    0x81: "F18", 0x82: "F19", 0x83: "F20",
}

LINE = re.compile(
    r"^\s*(?P<t>\d+\.\d+)s\s+(?P<kind>DOWN|UP|other)\s+"
    r"(?:(?P<vk>\S+)\s+scan=0x(?P<scan>[0-9A-Fa-f]+)\s+msg=0x(?P<msg>[0-9A-Fa-f]+)|"
    r"dwType=(?P<dtype>\d+)\s+raw=(?P<raw>[0-9A-Fa-f ]+))"
    r"\s+dev=(?P<dev>.+)$"
)


def decode_hid(raw_hex):
    data = bytes(int(x, 16) for x in raw_hex.split())
    if len(data) < 32:
        return None
    dw_type, dw_size = struct.unpack_from("<II", data, 0)
    if dw_type != 2:
        return None
    size_hid, count = struct.unpack_from("<II", data, 24)
    payload = data[32:32 + size_hid * count]
    if not payload:
        return None
    report_id = payload[0]
    usage = int.from_bytes(payload[1:3], "little") if len(payload) >= 3 else 0
    return report_id, usage


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("log")
    ap.add_argument("--all", action="store_true", help="include non-pad devices")
    args = ap.parse_args()

    rows = []
    for line in open(args.log, encoding="utf-8"):
        m = LINE.match(line.rstrip("\n"))
        if not m:
            continue
        dev = m.group("dev")
        if not args.all and PAD not in dev:
            continue
        t = float(m.group("t"))
        kind = m.group("kind")
        if kind in ("DOWN", "UP"):
            vk = m.group("vk")
            rows.append((t, kind, vk or "?", f"scan=0x{m.group('scan')}"))
        else:
            decoded = decode_hid(m.group("raw"))
            if not decoded:
                continue
            report_id, usage = decoded
            name = CONSUMER.get(usage, f"0x{usage:04X}")
            rows.append((t, "DOWN" if usage else "UP", name, f"report={report_id}"))

    if not rows:
        print("no pad events in this capture")
        return 1

    prev_t = None
    for t, state, name, extra in rows:
        delta = f"+{t - prev_t:6.3f}" if prev_t is not None else "       "
        extra = f"  {extra}" if extra else ""
        print(f"{t:9.3f}s {delta}  {state:<4} {name}{extra}")
        prev_t = t

    print(f"\n{len(rows)} pad events")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
