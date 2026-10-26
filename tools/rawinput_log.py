"""Log raw keyboard/consumer input per device, with the originating HID path.

This is the same channel the real app will use: Raw Input in the background,
which does not steal keys from the focused window and tells us *which* keyboard
sent them.

Usage:
    python tools/rawinput_log.py            # 30 s
    python tools/rawinput_log.py -s 120
    python tools/rawinput_log.py --until logs/stop
"""

import argparse
import ctypes
import ctypes.wintypes as wt
import datetime
import os
import sys
import time

user32 = ctypes.WinDLL("user32", use_last_error=True)
kernel32 = ctypes.WinDLL("kernel32", use_last_error=True)

WM_INPUT = 0x00FF
RID_INPUT = 0x10000003
RIDI_DEVICENAME = 0x20000007
RIM_TYPEKEYBOARD = 1
RIDEV_INPUTSINK = 0x00000100

VK_NAMES = {
    0x08: "Backspace", 0x09: "Tab", 0x0D: "Enter", 0x10: "Shift", 0x11: "Ctrl",
    0x12: "Alt", 0x13: "Pause", 0x14: "CapsLock", 0x1B: "Esc", 0x20: "Space",
    0x21: "PageUp", 0x22: "PageDown", 0x23: "End", 0x24: "Home",
    0x25: "Left", 0x26: "Up", 0x27: "Right", 0x28: "Down", 0x2C: "PrintScreen",
    0x2D: "Insert", 0x2E: "Delete", 0x5B: "LWin", 0x5C: "RWin", 0x5D: "Menu",
    0x90: "NumLock", 0x91: "ScrollLock",
}
for i in range(1, 25):
    VK_NAMES[0x6F + i] = f"F{i}"
for i in range(10):
    VK_NAMES[0x60 + i] = f"Num{i}"
VK_NAMES.update({
    0xA0: "LShift", 0xA1: "RShift", 0xA2: "LCtrl", 0xA3: "RCtrl",
    0xA4: "LAlt", 0xA5: "RAlt", 0xBA: ";", 0xBB: "=", 0xBC: ",", 0xBD: "-",
    0xBE: ".", 0xBF: "/", 0xC0: "`", 0xDB: "[", 0xDC: "\\", 0xDD: "]",
    0xDE: "'",
})


def vk_name(vk):
    if vk in VK_NAMES:
        return VK_NAMES[vk]
    if 0x30 <= vk <= 0x39 or 0x41 <= vk <= 0x5A:
        return chr(vk)
    return f"VK_0x{vk:02X}"


class RAWINPUTDEVICE(ctypes.Structure):
    _fields_ = [
        ("usUsagePage", wt.USHORT),
        ("usUsage", wt.USHORT),
        ("dwFlags", wt.DWORD),
        ("hwndTarget", wt.HWND),
    ]


class RAWINPUTHEADER(ctypes.Structure):
    _fields_ = [
        ("dwType", wt.DWORD),
        ("dwSize", wt.DWORD),
        ("hDevice", wt.HANDLE),
        ("wParam", wt.WPARAM),
    ]


class RAWKEYBOARD(ctypes.Structure):
    _fields_ = [
        ("MakeCode", wt.USHORT),
        ("Flags", wt.USHORT),
        ("Reserved", wt.USHORT),
        ("VKey", wt.USHORT),
        ("Message", wt.UINT),
        ("ExtraInformation", ctypes.c_ulong),
    ]


class RAWMOUSE(ctypes.Structure):
    _fields_ = [
        ("usFlags", wt.USHORT),
        ("ulButtons", wt.ULONG),
        ("ulRawButtons", wt.ULONG),
        ("lLastX", ctypes.c_long),
        ("lLastY", ctypes.c_long),
        ("ulExtraInformation", wt.ULONG),
    ]


class RAWHID(ctypes.Structure):
    _fields_ = [
        ("dwSizeHid", wt.DWORD),
        ("dwCount", wt.DWORD),
        ("bRawData", ctypes.c_ubyte * 1),
    ]


class RAWINPUT(ctypes.Structure):
    class _U(ctypes.Union):
        _fields_ = [("keyboard", RAWKEYBOARD), ("mouse", RAWMOUSE), ("hid", RAWHID)]

    _fields_ = [("header", RAWINPUTHEADER), ("data", _U)]


def device_name(handle):
    size = wt.UINT(0)
    user32.GetRawInputDeviceInfoW(handle, RIDI_DEVICENAME, None, ctypes.byref(size))
    if not size.value:
        return "?"
    buf = ctypes.create_unicode_buffer(size.value + 1)
    user32.GetRawInputDeviceInfoW(handle, RIDI_DEVICENAME, buf, ctypes.byref(size))
    return buf.value


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("-s", "--seconds", type=float, default=30)
    ap.add_argument("--until", default=None)
    ap.add_argument("-o", "--out", default=None)
    args = ap.parse_args()

    os.makedirs("logs", exist_ok=True)
    out_path = args.out or os.path.join(
        "logs", datetime.datetime.now().strftime("rawinput-%Y%m%d-%H%M%S.log")
    )

    user32.RegisterRawInputDevices.argtypes = [
        ctypes.c_void_p, wt.UINT, wt.UINT
    ]
    user32.RegisterRawInputDevices.restype = wt.BOOL
    user32.GetRawInputData.argtypes = [
        wt.HANDLE, wt.UINT, ctypes.c_void_p, ctypes.POINTER(wt.UINT), wt.UINT
    ]
    user32.GetRawInputData.restype = wt.UINT
    user32.DefWindowProcW.argtypes = [wt.HWND, wt.UINT, wt.WPARAM, wt.LPARAM]
    user32.DefWindowProcW.restype = ctypes.c_ssize_t
    user32.RegisterClassExW.argtypes = [ctypes.c_void_p]
    user32.RegisterClassExW.restype = wt.WORD
    user32.CreateWindowExW.argtypes = [
        wt.DWORD, wt.LPCWSTR, wt.LPCWSTR, wt.DWORD, ctypes.c_int, ctypes.c_int,
        ctypes.c_int, ctypes.c_int, wt.HWND, wt.HMENU, wt.HINSTANCE, ctypes.c_void_p,
    ]
    user32.CreateWindowExW.restype = wt.HWND
    user32.PeekMessageW.argtypes = [
        ctypes.POINTER(wt.MSG), wt.HWND, wt.UINT, wt.UINT, wt.UINT
    ]
    user32.PeekMessageW.restype = wt.BOOL
    user32.DestroyWindow.argtypes = [wt.HWND]
    user32.DestroyWindow.restype = wt.BOOL
    kernel32.GetModuleHandleW.restype = wt.HINSTANCE
    getname = user32.GetRawInputDeviceInfoW
    getname.argtypes = [wt.HANDLE, wt.UINT, ctypes.c_void_p, ctypes.POINTER(wt.UINT)]
    getname.restype = wt.UINT


    # Hidden top-level window: some Windows versions do not deliver raw input to
    # message-only windows.
    hinst = kernel32.GetModuleHandleW(None)
    wndproc_type = ctypes.WINFUNCTYPE(
        ctypes.c_ssize_t, wt.HWND, wt.UINT, wt.WPARAM, wt.LPARAM
    )

    messages = []

    @wndproc_type
    def wndproc(hwnd, msg, wparam, lparam):
        if msg == WM_INPUT:
            size = wt.UINT(0)
            user32.GetRawInputData(lparam, RID_INPUT, None, ctypes.byref(size),
                                   ctypes.sizeof(RAWINPUTHEADER))
            buf = ctypes.create_string_buffer(size.value)
            got = user32.GetRawInputData(lparam, RID_INPUT, buf, ctypes.byref(size),
                                         ctypes.sizeof(RAWINPUTHEADER))
            if got:
                ri = ctypes.cast(buf, ctypes.POINTER(RAWINPUT)).contents
                messages.append((time.perf_counter(), ri.header.hDevice, bytes(buf[:got])))
        return user32.DefWindowProcW(hwnd, msg, wparam, lparam)

    class_name = "MacroPadRawInputProbe"

    class WNDCLASSEXW(ctypes.Structure):
        _fields_ = [
            ("cbSize", wt.UINT), ("style", wt.UINT), ("lpfnWndProc", wndproc_type),
            ("cbClsExtra", ctypes.c_int), ("cbWndExtra", ctypes.c_int),
            ("hInstance", wt.HINSTANCE), ("hIcon", wt.HICON), ("hCursor", wt.HANDLE),
            ("hbrBackground", wt.HBRUSH), ("lpszMenuName", wt.LPCWSTR),
            ("lpszClassName", wt.LPCWSTR), ("hIconSm", wt.HICON),
        ]

    wcx = WNDCLASSEXW()
    wcx.cbSize = ctypes.sizeof(WNDCLASSEXW)
    wcx.lpfnWndProc = wndproc
    wcx.hInstance = hinst
    wcx.lpszClassName = class_name
    if not user32.RegisterClassExW(ctypes.byref(wcx)):
        raise ctypes.WinError(ctypes.get_last_error())

    hwnd = user32.CreateWindowExW(
        0, class_name, "probe", 0, 0, 0, 0, 0, None, None, hinst, None
    )
    if not hwnd:
        raise ctypes.WinError(ctypes.get_last_error())

    devs = (RAWINPUTDEVICE * 2)()
    devs[0].usUsagePage, devs[0].usUsage = 1, 6       # keyboard
    devs[0].dwFlags, devs[0].hwndTarget = RIDEV_INPUTSINK, hwnd
    devs[1].usUsagePage, devs[1].usUsage = 12, 1      # consumer
    devs[1].dwFlags, devs[1].hwndTarget = RIDEV_INPUTSINK, hwnd
    if not user32.RegisterRawInputDevices(ctypes.cast(devs, ctypes.c_void_p), 2, ctypes.sizeof(RAWINPUTDEVICE)):
        raise ctypes.WinError(ctypes.get_last_error())

    print(f"listening (raw input, background) -> {out_path}")
    print("press / hold / release the pad's keys and knob now...")

    t0 = time.perf_counter()
    last_handle = None
    stop_file = args.until
    msg = wt.MSG()
    with open(out_path, "w", encoding="utf-8") as out:
        out.write(
            f"# raw input capture started {datetime.datetime.now().isoformat(timespec='seconds')}"
            f"  limit={args.seconds:.0f}s  stop_file={stop_file}\n"
        )
        out.flush()
        while time.perf_counter() - t0 < args.seconds:
            if stop_file and os.path.exists(stop_file):
                break
            while user32.PeekMessageW(ctypes.byref(msg), None, 0, 0, 1):
                user32.TranslateMessage(ctypes.byref(msg))
                user32.DispatchMessageW(ctypes.byref(msg))

            while messages:
                t, handle, raw = messages.pop(0)
                name = device_name(handle) if handle else "?"
                ri = ctypes.cast(raw, ctypes.POINTER(RAWINPUT)).contents
                age = t - t0
                if ri.header.dwType == RIM_TYPEKEYBOARD:
                    kb = ri.data.keyboard
                    state = "UP" if (kb.Flags & 0x01) else "DOWN"
                    e0 = " E0" if (kb.Flags & 0x02) else ""
                    line = (
                        f"{age:8.3f}s {state:<4} vk={vk_name(kb.VKey):<10} "
                        f"scan=0x{kb.MakeCode:02X}{e0} msg=0x{kb.Message:04X} "
                        f"dev={name}"
                    )
                    if name != last_handle:
                        last_handle = name
                    print(line, flush=True)
                    out.write(line + "\n")
                else:
                    line = f"{age:8.3f}s other dwType={ri.header.dwType} raw={raw.hex(' ')} dev={name}"
                    print(line, flush=True)
                    out.write(line + "\n")
                out.flush()
            time.sleep(0.005)

    user32.DestroyWindow(hwnd)
    print(f"\nsaved: {out_path}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
