"""Dump HID interface + report descriptor info for the macro pad.

Usage: python tools/probe_hid.py [vid] [pid]
Defaults to the CH57x macro pad (0x1189 / 0x8890).
"""

import ctypes
import ctypes.wintypes as wt
import sys

setupapi = ctypes.WinDLL("setupapi", use_last_error=True)
hid = ctypes.WinDLL("hid", use_last_error=True)
kernel32 = ctypes.WinDLL("kernel32", use_last_error=True)

DIGCF_PRESENT = 0x02
DIGCF_DEVICEINTERFACE = 0x10
INVALID_HANDLE_VALUE = wt.HANDLE(-1).value
GENERIC_READ = 0x80000000
GENERIC_WRITE = 0x40000000
FILE_SHARE_READ = 1
FILE_SHARE_WRITE = 2
OPEN_EXISTING = 3
IOCTL_HID_GET_REPORT_DESCRIPTOR = 0x000B0000


class GUID(ctypes.Structure):
    _fields_ = [
        ("Data1", wt.DWORD),
        ("Data2", wt.WORD),
        ("Data3", wt.WORD),
        ("Data4", ctypes.c_ubyte * 8),
    ]


class SP_DEVICE_INTERFACE_DATA(ctypes.Structure):
    _fields_ = [
        ("cbSize", wt.DWORD),
        ("InterfaceClassGuid", GUID),
        ("Flags", wt.DWORD),
        ("Reserved", ctypes.POINTER(ctypes.c_ulong)),
    ]


class HIDD_ATTRIBUTES(ctypes.Structure):
    _fields_ = [
        ("Size", ctypes.c_ulong),
        ("VendorID", ctypes.c_ushort),
        ("ProductID", ctypes.c_ushort),
        ("VersionNumber", ctypes.c_ushort),
    ]


class HIDP_CAPS(ctypes.Structure):
    _fields_ = [
        ("Usage", ctypes.c_ushort),
        ("UsagePage", ctypes.c_ushort),
        ("InputReportByteLength", ctypes.c_ushort),
        ("OutputReportByteLength", ctypes.c_ushort),
        ("FeatureReportByteLength", ctypes.c_ushort),
        ("Reserved", ctypes.c_ushort * 17),
        ("NumberLinkCollectionNodes", ctypes.c_ushort),
        ("NumberInputButtonCaps", ctypes.c_ushort),
        ("NumberInputValueCaps", ctypes.c_ushort),
        ("NumberInputDataIndices", ctypes.c_ushort),
        ("NumberOutputButtonCaps", ctypes.c_ushort),
        ("NumberOutputValueCaps", ctypes.c_ushort),
        ("NumberOutputDataIndices", ctypes.c_ushort),
        ("NumberFeatureButtonCaps", ctypes.c_ushort),
        ("NumberFeatureValueCaps", ctypes.c_ushort),
        ("NumberFeatureDataIndices", ctypes.c_ushort),
    ]


def configure_signatures():
    setupapi.SetupDiGetClassDevsW.argtypes = [
        ctypes.POINTER(GUID), wt.LPCWSTR, wt.HWND, wt.DWORD
    ]
    setupapi.SetupDiGetClassDevsW.restype = wt.HANDLE
    setupapi.SetupDiEnumDeviceInterfaces.argtypes = [
        wt.HANDLE, ctypes.c_void_p, ctypes.POINTER(GUID), wt.DWORD,
        ctypes.POINTER(SP_DEVICE_INTERFACE_DATA),
    ]
    setupapi.SetupDiEnumDeviceInterfaces.restype = wt.BOOL
    setupapi.SetupDiGetDeviceInterfaceDetailW.argtypes = [
        wt.HANDLE, ctypes.POINTER(SP_DEVICE_INTERFACE_DATA), ctypes.c_void_p,
        wt.DWORD, ctypes.POINTER(wt.DWORD), ctypes.c_void_p,
    ]
    setupapi.SetupDiGetDeviceInterfaceDetailW.restype = wt.BOOL
    setupapi.SetupDiDestroyDeviceInfoList.argtypes = [wt.HANDLE]
    setupapi.SetupDiDestroyDeviceInfoList.restype = wt.BOOL

    hid.HidD_GetAttributes.argtypes = [wt.HANDLE, ctypes.POINTER(HIDD_ATTRIBUTES)]
    hid.HidD_GetAttributes.restype = wt.BOOL
    hid.HidD_GetPreparsedData.argtypes = [wt.HANDLE, ctypes.POINTER(ctypes.c_void_p)]
    hid.HidD_GetPreparsedData.restype = wt.BOOL
    hid.HidD_FreePreparsedData.argtypes = [ctypes.c_void_p]
    hid.HidD_FreePreparsedData.restype = wt.BOOL
    hid.HidP_GetCaps.argtypes = [ctypes.c_void_p, ctypes.POINTER(HIDP_CAPS)]
    hid.HidP_GetCaps.restype = ctypes.c_long
    hid.HidD_GetManufacturerString.argtypes = [wt.HANDLE, ctypes.c_void_p, ctypes.c_ulong]
    hid.HidD_GetProductString.argtypes = [wt.HANDLE, ctypes.c_void_p, ctypes.c_ulong]
    hid.HidD_GetSerialNumberString.argtypes = [wt.HANDLE, ctypes.c_void_p, ctypes.c_ulong]

    kernel32.CreateFileW.argtypes = [
        wt.LPCWSTR, wt.DWORD, wt.DWORD, ctypes.c_void_p, wt.DWORD, wt.DWORD, wt.HANDLE
    ]
    kernel32.CreateFileW.restype = wt.HANDLE
    kernel32.CloseHandle.argtypes = [wt.HANDLE]
    kernel32.DeviceIoControl.argtypes = [
        wt.HANDLE, wt.DWORD, ctypes.c_void_p, wt.DWORD, ctypes.c_void_p,
        wt.DWORD, ctypes.POINTER(wt.DWORD), ctypes.c_void_p,
    ]
    kernel32.DeviceIoControl.restype = wt.BOOL


def hid_string(handle, getter):
    buf = ctypes.create_unicode_buffer(512)
    if getter(handle, buf, ctypes.sizeof(buf)):
        return buf.value
    return None


def report_descriptor(handle):
    buf = (ctypes.c_ubyte * 4096)()
    returned = wt.DWORD(0)
    ok = kernel32.DeviceIoControl(
        handle, IOCTL_HID_GET_REPORT_DESCRIPTOR, None, 0,
        buf, ctypes.sizeof(buf), ctypes.byref(returned), None,
    )
    if not ok:
        return None
    return bytes(buf[: returned.value])


def decode_descriptor(data):
    """Very small HID item decoder good enough to read the layout."""
    out = []
    i = 0
    usage_page = 0
    report_id = None
    report_size = 0
    report_count = 0
    while i < len(data):
        prefix = data[i]
        if prefix == 0xFE:  # long item
            size = data[i + 1] if i + 1 < len(data) else 0
            i += 3 + size
            continue
        size = (0, 1, 2, 4)[prefix & 0x03]
        typ = (prefix >> 2) & 0x03
        tag = prefix >> 4
        value = int.from_bytes(data[i + 1 : i + 1 + size], "little") if size else 0
        i += 1 + size

        if typ == 1:  # global
            if tag == 0x0:
                usage_page = value
            elif tag == 0x7:
                report_size = value
            elif tag == 0x8:
                report_id = value
            elif tag == 0x9:
                report_count = value
        elif typ == 2 and tag == 0x0:  # local usage
            out.append(f"  UsagePage=0x{usage_page:04X} Usage=0x{value:04X}")
        elif typ == 0:
            names = {0x8: "Input", 0x9: "Output", 0xB: "Feature"}
            if tag in names:
                rid = f" id={report_id}" if report_id is not None else ""
                out.append(
                    f"  {names[tag]}{rid}: size={report_size} count={report_count} "
                    f"flags=0x{value:02X}"
                )
    return out


def main():
    vid = int(sys.argv[1], 0) if len(sys.argv) > 1 else 0x1189
    pid = int(sys.argv[2], 0) if len(sys.argv) > 2 else 0x8890
    configure_signatures()

    # GUID_DEVINTERFACE_HID
    hid_guid = GUID(
        0x4D1E55B2, 0xF16F, 0x11CF,
        (ctypes.c_ubyte * 8)(0x88, 0xCB, 0x00, 0x11, 0x11, 0x00, 0x00, 0x30),
    )

    devinfo = setupapi.SetupDiGetClassDevsW(
        ctypes.byref(hid_guid), None, None, DIGCF_PRESENT | DIGCF_DEVICEINTERFACE
    )
    if devinfo == INVALID_HANDLE_VALUE:
        raise ctypes.WinError(ctypes.get_last_error())

    iface = SP_DEVICE_INTERFACE_DATA()
    iface.cbSize = ctypes.sizeof(SP_DEVICE_INTERFACE_DATA)

    idx = 0
    try:
        while setupapi.SetupDiEnumDeviceInterfaces(
            devinfo, None, ctypes.byref(hid_guid), idx, ctypes.byref(iface)
        ):
            idx += 1
            needed = wt.DWORD(0)
            setupapi.SetupDiGetDeviceInterfaceDetailW(
                devinfo, ctypes.byref(iface), None, 0, ctypes.byref(needed), None
            )
            buf = ctypes.create_string_buffer(needed.value)
            # cbSize is 8 on 64-bit (DWORD + WCHAR[1] padded)
            ctypes.cast(buf, ctypes.POINTER(wt.DWORD))[0] = 8 if ctypes.sizeof(ctypes.c_void_p) == 8 else 6
            if not setupapi.SetupDiGetDeviceInterfaceDetailW(
                devinfo, ctypes.byref(iface), buf, needed.value,
                ctypes.byref(needed), None,
            ):
                continue
            path = ctypes.wstring_at(ctypes.addressof(buf) + 4)

            handle = kernel32.CreateFileW(
                path, GENERIC_READ | GENERIC_WRITE,
                FILE_SHARE_READ | FILE_SHARE_WRITE, None, OPEN_EXISTING, 0, None,
            )
            if handle == INVALID_HANDLE_VALUE:
                handle = kernel32.CreateFileW(
                    path, 0, FILE_SHARE_READ | FILE_SHARE_WRITE,
                    None, OPEN_EXISTING, 0, None,
                )
            if handle == INVALID_HANDLE_VALUE:
                continue
            try:
                attrs = HIDD_ATTRIBUTES()
                attrs.Size = ctypes.sizeof(HIDD_ATTRIBUTES)
                if not hid.HidD_GetAttributes(handle, ctypes.byref(attrs)):
                    continue
                if (attrs.VendorID, attrs.ProductID) != (vid, pid):
                    continue

                print(f"\n=== {path}")
                print(
                    f"vendor=0x{attrs.VendorID:04X} product=0x{attrs.ProductID:04X} "
                    f"version=0x{attrs.VersionNumber:04X}"
                )
                for label, getter in (
                    ("manufacturer", hid.HidD_GetManufacturerString),
                    ("product", hid.HidD_GetProductString),
                    ("serial", hid.HidD_GetSerialNumberString),
                ):
                    print(f"{label}: {hid_string(handle, getter)!r}")

                preparsed = ctypes.c_void_p()
                if hid.HidD_GetPreparsedData(handle, ctypes.byref(preparsed)):
                    try:
                        caps = HIDP_CAPS()
                        hid.HidP_GetCaps(preparsed, ctypes.byref(caps))
                        print(
                            f"usage_page=0x{caps.UsagePage:04X} usage=0x{caps.Usage:04X} "
                            f"in={caps.InputReportByteLength} out={caps.OutputReportByteLength} "
                            f"feature={caps.FeatureReportByteLength} "
                            f"in_buttons={caps.NumberInputButtonCaps} "
                            f"in_values={caps.NumberInputValueCaps}"
                        )
                    finally:
                        hid.HidD_FreePreparsedData(preparsed)

                desc = report_descriptor(handle)
                if desc:
                    print(f"report_descriptor ({len(desc)} bytes):")
                    print("  hex:", " ".join(f"{b:02X}" for b in desc))
                    for line in decode_descriptor(desc):
                        print(line)
                else:
                    print("report_descriptor: unavailable")
            finally:
                kernel32.CloseHandle(handle)
    finally:
        setupapi.SetupDiDestroyDeviceInfoList(devinfo)

    if idx == 0:
        print("No HID interfaces found.")


if __name__ == "__main__":
    main()
