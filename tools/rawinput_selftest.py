"""Self-test: prove that Raw Input sees synthesized keys, without touching the pad."""

import ctypes
import ctypes.wintypes as wt
import subprocess
import sys
import time
import os

user32 = ctypes.WinDLL("user32", use_last_error=True)

ULONG_PTR = ctypes.c_ulonglong


class KEYBDINPUT(ctypes.Structure):
    _fields_ = [
        ("wVk", wt.WORD),
        ("wScan", wt.WORD),
        ("dwFlags", wt.DWORD),
        ("time", wt.DWORD),
        ("dwExtraInfo", ULONG_PTR),
    ]


class MOUSEINPUT(ctypes.Structure):
    _fields_ = [
        ("dx", ctypes.c_long), ("dy", ctypes.c_long), ("mouseData", wt.DWORD),
        ("dwFlags", wt.DWORD), ("time", wt.DWORD), ("dwExtraInfo", ULONG_PTR),
    ]


class HARDWAREINPUT(ctypes.Structure):
    _fields_ = [("uMsg", wt.DWORD), ("wParamL", wt.WORD), ("wParamH", wt.WORD)]


class INPUT(ctypes.Union):
    _fields_ = [("ki", KEYBDINPUT), ("mi", MOUSEINPUT), ("hi", HARDWAREINPUT)]


class INPUTUNION(ctypes.Structure):
    _anonymous_ = ("u",)
    _fields_ = [("type", wt.DWORD), ("u", INPUT)]


user32.SendInput.argtypes = [wt.UINT, ctypes.POINTER(INPUTUNION), ctypes.c_int]
user32.SendInput.restype = wt.UINT


def tap(vk):
    inputs = (INPUTUNION * 2)()
    inputs[0].type = 1
    inputs[0].ki.wVk = vk
    inputs[1].type = 1
    inputs[1].ki.wVk = vk
    inputs[1].ki.dwFlags = 0x0002  # KEYEVENTF_KEYUP
    user32.SendInput(2, inputs, ctypes.sizeof(INPUTUNION))


log = "logs/selftest-rawinput.log"
if os.path.exists(log):
    os.remove(log)

proc = subprocess.Popen(
    [sys.executable, "tools/rawinput_log.py", "-s", "6", "-o", log],
    stdout=subprocess.DEVNULL,
    stderr=subprocess.STDOUT,
)
time.sleep(2.0)
tap(0x7C)  # F13
time.sleep(0.3)
tap(0x7D)  # F14
proc.wait()

print(open(log, encoding="utf-8").read() or "(no events captured)")
