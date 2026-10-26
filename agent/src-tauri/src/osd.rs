use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use serde_json::json;
use tauri::{AppHandle, Manager, PhysicalPosition, WebviewWindow};
use windows_sys::Win32::Foundation::{HWND, RECT};
use windows_sys::Win32::Graphics::Gdi::{
    GetMonitorInfoW, MonitorFromWindow, MONITORINFO, MONITOR_DEFAULTTONEAREST,
    MONITOR_DEFAULTTOPRIMARY,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    GetForegroundWindow, GetWindowLongPtrW, SetWindowLongPtrW, GWL_EXSTYLE, WS_EX_NOACTIVATE,
    WS_EX_TOOLWINDOW, WS_EX_TRANSPARENT,
};

static GENERATION: AtomicU64 = AtomicU64::new(0);

const MARGIN: i32 = 48;

pub fn init(win: &WebviewWindow) {
    let _ = win.set_ignore_cursor_events(true);
    if let Ok(hwnd) = win.hwnd() {
        let hwnd = hwnd.0 as isize as HWND;
        unsafe {
            let ex = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
            let extra = (WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW | WS_EX_TRANSPARENT) as isize;
            SetWindowLongPtrW(hwnd, GWL_EXSTYLE, ex | extra);
        }
    }
}

fn work_area(monitor: &str) -> RECT {
    unsafe {
        let hwnd = if monitor.eq_ignore_ascii_case("primary") {
            std::ptr::null_mut()
        } else {
            GetForegroundWindow()
        };
        let mut monitor = MonitorFromWindow(
            hwnd,
            if hwnd.is_null() {
                MONITOR_DEFAULTTOPRIMARY
            } else {
                MONITOR_DEFAULTTONEAREST
            },
        );
        if monitor.is_null() {
            monitor = MonitorFromWindow(std::ptr::null_mut(), MONITOR_DEFAULTTOPRIMARY);
        }
        let mut info: MONITORINFO = std::mem::zeroed();
        info.cbSize = std::mem::size_of::<MONITORINFO>() as u32;
        if GetMonitorInfoW(monitor, &mut info) != 0 {
            info.rcWork
        } else {
            RECT {
                left: 0,
                top: 0,
                right: 1920,
                bottom: 1080,
            }
        }
    }
}

fn place(win: &WebviewWindow, monitor: &str) {
    let Ok(size) = win.outer_size() else {
        return;
    };
    let work = work_area(monitor);
    let width = work.right - work.left;
    let x = work.left + (width - size.width as i32) / 2;
    let y = work.bottom - size.height as i32 - MARGIN;
    crate::log_info!(
        "[osd] work={},{}..{},{} size={}x{} -> {x},{y}",
        work.left,
        work.top,
        work.right,
        work.bottom,
        size.width,
        size.height
    );
    let _ = win.set_position(PhysicalPosition::new(x, y.max(work.top + MARGIN)));
}

pub fn show(
    app: &AppHandle,
    payload: serde_json::Value,
    duration_ms: u64,
    sticky: bool,
    monitor: &str,
) {
    // Claim this show before touching the window. Any hide timer that was
    // started for a previous OSD will then abort during its fade.
    let generation = GENERATION.fetch_add(1, Ordering::SeqCst) + 1;

    let Some(win) = app.get_webview_window("osd") else {
        return;
    };
    place(&win, monitor);
    let js = format!(
        "window.showOsd({});",
        serde_json::to_string(&payload).unwrap_or_else(|_| "{}".into())
    );
    let _ = win.eval(&js);
    let _ = win.show();

    if sticky || duration_ms == 0 {
        return;
    }

    let app = app.clone();
    std::thread::spawn(move || {
        let fade_after = duration_ms.saturating_sub(220);
        std::thread::sleep(Duration::from_millis(fade_after));
        if GENERATION.load(Ordering::SeqCst) != generation {
            return;
        }
        if let Some(win) = app.get_webview_window("osd") {
            let _ = win.eval("window.fadeOsd && window.fadeOsd();");
        }
        std::thread::sleep(Duration::from_millis(220));
        if GENERATION.load(Ordering::SeqCst) != generation {
            return;
        }
        if let Some(win) = app.get_webview_window("osd") {
            let _ = win.hide();
        }
    });
}

pub fn hide(app: &AppHandle) {
    // Do not block the engine thread for the fade. The generation check makes
    // a late hide harmless if a new press has already shown the OSD again.
    let generation = GENERATION.fetch_add(1, Ordering::SeqCst) + 1;
    let Some(win) = app.get_webview_window("osd") else {
        return;
    };
    let _ = win.eval("window.fadeOsd && window.fadeOsd();");

    let app = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(180));
        if GENERATION.load(Ordering::SeqCst) != generation {
            return;
        }
        if let Some(win) = app.get_webview_window("osd") {
            let _ = win.hide();
        }
    });
}

pub struct Payload<'a> {
    pub context: &'a str,
    pub layer: &'a str,
    pub color: &'a str,
    pub keys: [&'a str; 3],
    pub knob: [&'a str; 3],
    pub badge: Option<&'a str>,
    pub pressed: Option<&'a str>,
    pub action: Option<&'a str>,
    pub rail: serde_json::Value,
    pub mode: &'a str,
    pub gestures: serde_json::Value,
}

impl Payload<'_> {
    pub fn into_json(self) -> serde_json::Value {
        json!({
            "context": self.context,
            "layer": self.layer,
            "color": self.color,
            "keys": self.keys,
            "knob": self.knob,
            "badge": self.badge,
            "pressed": self.pressed,
            "action": self.action,
            "rail": self.rail,
            "mode": self.mode,
            "gestures": self.gestures,
        })
    }
}

pub fn layer_payload(
    runtime: &crate::config::Runtime,
    context_id: &str,
    layer_name: &str,
    badge: Option<&str>,
    pressed: Option<&str>,
    action: Option<&str>,
    mode: &str,
) -> serde_json::Value {
    let context = runtime.context_label(context_id);
    let rail: Vec<serde_json::Value> = runtime
        .context_layers(context_id)
        .into_iter()
        .map(|id| {
            let label = runtime
                .layer(&id)
                .map(|l| l.title(&id))
                .unwrap_or_else(|| id.clone());
            json!({
                "id": id,
                "label": label,
                "active": id == layer_name,
            })
        })
        .collect();

    let layer = runtime.layer(layer_name);
    let label = layer
        .map(|l| l.title(layer_name))
        .unwrap_or_else(|| layer_name.to_string());
    let color = layer.map(|l| l.color()).unwrap_or_else(|| "#7dd3fc".into());

    let get = |input: crate::config::Input| {
        layer
            .and_then(|l| l.binding(input))
            .and_then(|b| b.tap())
            .map(|a| a.label())
            .unwrap_or_else(|| "—".into())
    };

    let gesture = |input: crate::config::Input, kind: &str| -> String {
        let binding = layer.and_then(|l| l.binding(input));
        let action = match kind {
            "tap" => binding.and_then(|b| b.tap()),
            "double" => binding.and_then(|b| b.double()),
            "hold" => binding.and_then(|b| b.hold()),
            _ => None,
        };
        action.map(|a| a.label()).unwrap_or_default()
    };

    let gestures = json!({
        "k1": {"tap": gesture(crate::config::Input::K1, "tap"), "double": gesture(crate::config::Input::K1, "double"), "hold": gesture(crate::config::Input::K1, "hold")},
        "k2": {"tap": gesture(crate::config::Input::K2, "tap"), "double": gesture(crate::config::Input::K2, "double"), "hold": gesture(crate::config::Input::K2, "hold")},
        "k3": {"tap": gesture(crate::config::Input::K3, "tap"), "double": gesture(crate::config::Input::K3, "double"), "hold": gesture(crate::config::Input::K3, "hold")},
        "knob_ccw": {"tap": gesture(crate::config::Input::KnobCcw, "tap"), "double": "", "hold": ""},
        "knob_press": {"tap": gesture(crate::config::Input::KnobPress, "tap"), "double": gesture(crate::config::Input::KnobPress, "double"), "hold": gesture(crate::config::Input::KnobPress, "hold")},
        "knob_cw": {"tap": gesture(crate::config::Input::KnobCw, "tap"), "double": "", "hold": ""}
    });

    let k1 = get(crate::config::Input::K1);
    let k2 = get(crate::config::Input::K2);
    let k3 = get(crate::config::Input::K3);
    let ccw = get(crate::config::Input::KnobCcw);
    let press = get(crate::config::Input::KnobPress);
    let cw = get(crate::config::Input::KnobCw);

    Payload {
        context: &context,
        layer: &label,
        color: &color,
        keys: [k1.as_str(), k2.as_str(), k3.as_str()],
        knob: [ccw.as_str(), press.as_str(), cw.as_str()],
        badge,
        pressed,
        action,
        rail: json!(rail),
        mode,
        gestures,
    }
    .into_json()
}
