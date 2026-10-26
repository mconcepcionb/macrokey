use std::path::Path;
use std::sync::mpsc::Sender;
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

use windows_sys::Win32::Foundation::{CloseHandle, HWND};
use windows_sys::Win32::System::Threading::{
    OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    GetClassNameW, GetForegroundWindow, GetWindowTextLengthW, GetWindowTextW,
    GetWindowThreadProcessId,
};

#[derive(Clone, Debug)]
pub struct FocusInfo {
    pub exe: String,
    pub title: String,
    pub class: String,
}

impl FocusInfo {
    fn key(&self) -> String {
        format!("{}|{}|{}", self.exe, self.title, self.class)
    }
}

unsafe fn window_title(hwnd: HWND) -> String {
    let len = GetWindowTextLengthW(hwnd);
    if len <= 0 {
        return String::new();
    }
    let mut buf = vec![0u16; len as usize + 1];
    let n = GetWindowTextW(hwnd, buf.as_mut_ptr(), buf.len() as i32);
    String::from_utf16_lossy(&buf[..n.max(0) as usize])
}

unsafe fn window_class(hwnd: HWND) -> String {
    let mut buf = vec![0u16; 256];
    let n = GetClassNameW(hwnd, buf.as_mut_ptr(), buf.len() as i32);
    String::from_utf16_lossy(&buf[..n.max(0) as usize])
}

unsafe fn process_name(hwnd: HWND) -> String {
    let mut pid = 0u32;
    GetWindowThreadProcessId(hwnd, &mut pid);
    if pid == 0 {
        return String::new();
    }
    let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
    if handle.is_null() {
        return String::new();
    }
    let mut buf = vec![0u16; 1024];
    let mut size = buf.len() as u32;
    let ok = QueryFullProcessImageNameW(handle, PROCESS_NAME_WIN32, buf.as_mut_ptr(), &mut size);
    CloseHandle(handle);
    if ok == 0 {
        return String::new();
    }
    let path = String::from_utf16_lossy(&buf[..size as usize]);
    Path::new(&path)
        .file_name()
        .map(|n| n.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default()
}

fn current_focus() -> Option<FocusInfo> {
    unsafe {
        let hwnd = GetForegroundWindow();
        if hwnd.is_null() {
            return None;
        }
        let exe = process_name(hwnd);
        if exe.is_empty() {
            return None;
        }
        Some(FocusInfo {
            exe,
            title: window_title(hwnd),
            class: window_class(hwnd),
        })
    }
}

static LAST_FOREIGN: OnceLock<Mutex<Option<FocusInfo>>> = OnceLock::new();

pub fn last_foreign() -> Option<FocusInfo> {
    LAST_FOREIGN
        .get()
        .and_then(|store| store.lock().ok().and_then(|guard| guard.as_ref().cloned()))
}

pub fn spawn(tx: Sender<FocusInfo>) {
    let _ = LAST_FOREIGN.set(Mutex::new(None));
    let own_exe = std::env::current_exe()
        .ok()
        .and_then(|path| {
            path.file_name()
                .map(|name| name.to_string_lossy().to_ascii_lowercase())
        })
        .unwrap_or_default();

    std::thread::spawn(move || {
        let mut last = String::new();
        loop {
            if let Some(focus) = current_focus() {
                let key = focus.key();
                if key != last {
                    last = key;
                    if !focus.exe.eq_ignore_ascii_case(&own_exe) {
                        if let Some(store) = LAST_FOREIGN.get() {
                            if let Ok(mut guard) = store.lock() {
                                *guard = Some(focus.clone());
                            }
                        }
                    }
                    if tx.send(focus).is_err() {
                        break;
                    }
                }
            }
            std::thread::sleep(Duration::from_millis(150));
        }
    });
}
