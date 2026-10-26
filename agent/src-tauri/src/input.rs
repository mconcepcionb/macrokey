use std::collections::HashMap;
use std::sync::mpsc::Sender;
use std::sync::{Mutex, OnceLock};
use std::time::Instant;

use windows_sys::Win32::Foundation::{HANDLE, HWND, LPARAM, LRESULT, WPARAM};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::Input::{
    GetRawInputData, GetRawInputDeviceInfoW, RegisterRawInputDevices, RAWINPUT, RAWINPUTDEVICE,
    RAWINPUTHEADER, RIDEV_INPUTSINK, RIDI_DEVICENAME, RID_INPUT, RIM_TYPEKEYBOARD,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DispatchMessageW, GetMessageW, RegisterClassExW, MSG,
    WM_INPUT, WNDCLASSEXW, WS_EX_TOOLWINDOW,
};

use crate::config::Input;

#[derive(Debug, Clone, Copy)]
pub struct InputEvent {
    pub input: Input,
    pub down: bool,
    pub at: Instant,
}

static TX: OnceLock<Mutex<Sender<InputEvent>>> = OnceLock::new();
static KEYMAP: OnceLock<HashMap<u16, Input>> = OnceLock::new();
static PAD_PATTERN: OnceLock<String> = OnceLock::new();

fn pad_device(name: &str) -> bool {
    match PAD_PATTERN.get() {
        Some(pattern) => name.to_ascii_lowercase().contains(pattern.as_str()),
        None => false,
    }
}

unsafe fn device_name(handle: HANDLE) -> String {
    let mut size: u32 = 0;
    GetRawInputDeviceInfoW(handle, RIDI_DEVICENAME, std::ptr::null_mut(), &mut size);
    if size == 0 {
        return String::new();
    }
    let mut buf = vec![0u16; size as usize];
    let copied = GetRawInputDeviceInfoW(
        handle,
        RIDI_DEVICENAME,
        buf.as_mut_ptr() as *mut std::ffi::c_void,
        &mut size,
    );
    if copied == 0 || copied == u32::MAX {
        return String::new();
    }
    let end = buf.iter().position(|c| *c == 0).unwrap_or(buf.len());
    String::from_utf16_lossy(&buf[..end])
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if msg == WM_INPUT {
        let mut size: u32 = 0;
        GetRawInputData(
            lparam as _,
            RID_INPUT,
            std::ptr::null_mut(),
            &mut size,
            std::mem::size_of::<RAWINPUTHEADER>() as u32,
        );
        if size > 0 {
            let mut buf = vec![0u8; size as usize];
            let copied = GetRawInputData(
                lparam as _,
                RID_INPUT,
                buf.as_mut_ptr() as *mut std::ffi::c_void,
                &mut size,
                std::mem::size_of::<RAWINPUTHEADER>() as u32,
            );
            if copied > 0 && copied != u32::MAX {
                let ri = &*(buf.as_ptr() as *const RAWINPUT);
                if ri.header.dwType == RIM_TYPEKEYBOARD {
                    let kb = &ri.data.keyboard;
                    let vk = kb.VKey;
                    if let Some(input) = KEYMAP.get().and_then(|m| m.get(&vk)).copied() {
                        let name = device_name(ri.header.hDevice);
                        if pad_device(&name) {
                            let event = InputEvent {
                                input,
                                down: (kb.Flags & 0x01) == 0,
                                at: Instant::now(),
                            };
                            if let Some(tx) = TX.get() {
                                if let Ok(tx) = tx.lock() {
                                    let _ = tx.send(event);
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    DefWindowProcW(hwnd, msg, wparam, lparam)
}

fn wide(value: &str) -> Vec<u16> {
    let mut v: Vec<u16> = value.encode_utf16().collect();
    v.push(0);
    v
}

unsafe fn run() {
    let class_name = wide("MacrokeyRawInput");
    let instance = GetModuleHandleW(std::ptr::null());

    let mut wc = WNDCLASSEXW {
        cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
        lpfnWndProc: Some(wndproc),
        hInstance: instance,
        lpszClassName: class_name.as_ptr(),
        ..Default::default()
    };

    if RegisterClassExW(&wc) == 0 {
        crate::fatal(
            "macrokey",
            "Could not create the Raw Input listener window. Restart macrokey.",
        );
        return;
    }

    let hwnd = CreateWindowExW(
        WS_EX_TOOLWINDOW,
        class_name.as_ptr(),
        wide("macrokey-input").as_ptr(),
        0,
        0,
        0,
        0,
        0,
        std::ptr::null_mut(),
        std::ptr::null_mut(),
        instance,
        std::ptr::null(),
    );
    if hwnd.is_null() {
        crate::fatal(
            "macrokey",
            "Could not create the Raw Input listener window. Restart macrokey.",
        );
        return;
    }

    let devices = [RAWINPUTDEVICE {
        usUsagePage: 0x01,
        usUsage: 0x06,
        dwFlags: RIDEV_INPUTSINK,
        hwndTarget: hwnd,
    }];
    if RegisterRawInputDevices(
        devices.as_ptr(),
        devices.len() as u32,
        std::mem::size_of::<RAWINPUTDEVICE>() as u32,
    ) == 0
    {
        crate::fatal(
            "macrokey",
            "Raw Input registration failed. The pad cannot be read. Restart macrokey.",
        );
        return;
    }

    let mut msg = MSG::default();
    while GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) > 0 {
        DispatchMessageW(&msg);
    }
    let _ = &mut wc;
}

pub fn spawn(tx: Sender<InputEvent>, keymap: HashMap<u16, Input>, vid: u32, pid: u32) {
    let _ = TX.set(Mutex::new(tx));
    let _ = KEYMAP.set(keymap);
    let _ = PAD_PATTERN.set(format!("vid_{vid:04x}&pid_{pid:04x}"));
    std::thread::spawn(|| unsafe { run() });
}
