use std::os::windows::ffi::OsStrExt;
use std::path::Path;

use anyhow::{bail, Result};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::*;
use windows_sys::Win32::UI::Shell::ShellExecuteW;
use windows_sys::Win32::UI::WindowsAndMessaging::*;

use crate::config::{Action, SequenceStep};

const WHEEL_DELTA: i32 = 120;
const MAGIC: usize = 0x4D41_4352; // "MACR"

// ---------------------------------------------------------------------------
// key names
// ---------------------------------------------------------------------------

pub fn key_to_vk(name: &str) -> Option<u16> {
    let raw = name.trim().to_ascii_lowercase();
    let n = raw.strip_prefix("vk_").unwrap_or(&raw);

    if let Some(rest) = n.strip_prefix('f') {
        if let Ok(num) = rest.parse::<u16>() {
            if (1..=24).contains(&num) {
                return Some(0x70 + num - 1);
            }
        }
    }
    if let Some(rest) = n.strip_prefix("num") {
        if let Ok(num) = rest.parse::<u16>() {
            if num <= 9 {
                return Some(0x60 + num);
            }
        }
    }
    if n.len() == 1 {
        let c = n.chars().next().unwrap();
        if c.is_ascii_lowercase() {
            return Some(c.to_ascii_uppercase() as u16);
        }
        if c.is_ascii_digit() {
            return Some(c as u16);
        }
    }

    Some(match n {
        "enter" | "return" => 0x0D,
        "esc" | "escape" => 0x1B,
        "space" => 0x20,
        "tab" => 0x09,
        "backspace" => 0x08,
        "capslock" => 0x14,
        "left" => 0x25,
        "up" => 0x26,
        "right" => 0x27,
        "down" => 0x28,
        "home" => 0x24,
        "end" => 0x23,
        "pageup" | "pgup" => 0x21,
        "pagedown" | "pgdn" => 0x22,
        "insert" | "ins" => 0x2D,
        "delete" | "del" => 0x2E,
        "printscreen" | "prtsc" => 0x2C,
        "scrolllock" => 0x91,
        "pause" => 0x13,
        "grave" | "backtick" => 0xC0,
        "minus" => 0xBD,
        "equal" => 0xBB,
        "leftbracket" => 0xDB,
        "rightbracket" => 0xDD,
        "backslash" => 0xDC,
        "semicolon" => 0xBA,
        "quote" => 0xDE,
        "comma" => 0xBC,
        "dot" | "period" => 0xBE,
        "slash" => 0xBF,
        "lctrl" | "ctrl" => 0xA2,
        "rctrl" => 0xA3,
        "lshift" | "shift" => 0xA0,
        "rshift" => 0xA1,
        "lalt" | "alt" | "opt" => 0xA4,
        "ralt" | "ropt" => 0xA5,
        "lwin" | "win" | "cmd" => 0x5B,
        "rwin" | "rcmd" => 0x5C,
        "play" | "playpause" | "play_pause" => 0xB3,
        "next" => 0xB0,
        "previous" | "prev" => 0xB1,
        "stop" => 0xB2,
        "mute" => 0xAD,
        "volumeup" | "volume_up" | "volup" => 0xAF,
        "volumedown" | "volume_down" | "voldown" => 0xAE,
        _ => return None,
    })
}

fn is_modifier(name: &str) -> bool {
    matches!(
        name.trim().to_ascii_lowercase().as_str(),
        "ctrl"
            | "lctrl"
            | "rctrl"
            | "shift"
            | "lshift"
            | "rshift"
            | "alt"
            | "lalt"
            | "ralt"
            | "opt"
            | "ropt"
            | "win"
            | "lwin"
            | "rwin"
            | "cmd"
            | "rcmd"
    )
}

fn extended(vk: u16) -> bool {
    matches!(
        vk,
        0x21..=0x2E | 0x5B | 0x5C | 0x6F | 0x90 | 0xAD..=0xB3 | 0xA3 | 0xA5
    )
}

fn kbd(vk: u16, up: bool) -> INPUT {
    let mut flags = 0u32;
    if up {
        flags |= KEYEVENTF_KEYUP;
    }
    if extended(vk) {
        flags |= KEYEVENTF_EXTENDEDKEY;
    }
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: vk,
                wScan: 0,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: MAGIC,
            },
        },
    }
}

fn mouse_wheel(delta: i32) -> INPUT {
    INPUT {
        r#type: INPUT_MOUSE,
        Anonymous: INPUT_0 {
            mi: MOUSEINPUT {
                dx: 0,
                dy: 0,
                mouseData: (delta * WHEEL_DELTA) as u32,
                dwFlags: MOUSEEVENTF_WHEEL,
                time: 0,
                dwExtraInfo: MAGIC,
            },
        },
    }
}

fn send(inputs: &[INPUT]) -> Result<()> {
    if inputs.is_empty() {
        return Ok(());
    }
    let sent = unsafe {
        SendInput(
            inputs.len() as u32,
            inputs.as_ptr(),
            std::mem::size_of::<INPUT>() as i32,
        )
    };
    if sent != inputs.len() as u32 {
        bail!(
            "SendInput inserted {} of {} events (blocked by UIPI?)",
            sent,
            inputs.len()
        );
    }
    Ok(())
}

pub fn send_hotkey(keys: &[String]) -> Result<()> {
    let mut mods = Vec::new();
    let mut main = Vec::new();
    for key in keys {
        let Some(vk) = key_to_vk(key) else {
            bail!("unknown key {key:?}");
        };
        if is_modifier(key) {
            mods.push(vk);
        } else {
            main.push(vk);
        }
    }
    if main.is_empty() {
        bail!("hotkey has no non-modifier key: {keys:?}");
    }

    let mut inputs = Vec::new();
    for vk in &mods {
        inputs.push(kbd(*vk, false));
    }
    for vk in &main {
        inputs.push(kbd(*vk, false));
        inputs.push(kbd(*vk, true));
    }
    for vk in mods.iter().rev() {
        inputs.push(kbd(*vk, true));
    }
    send(&inputs)
}

fn send_media(key: &str) -> Result<()> {
    let Some(vk) = key_to_vk(key) else {
        bail!("unknown media key {key:?}");
    };
    send(&[kbd(vk, false), kbd(vk, true)])
}

pub fn send_wheel(delta: i32, modifiers: &[String]) -> Result<()> {
    let mut mods = Vec::new();
    for m in modifiers {
        let Some(vk) = key_to_vk(m) else {
            bail!("unknown wheel modifier {m:?}");
        };
        mods.push(vk);
    }
    let mut inputs = Vec::new();
    for vk in &mods {
        inputs.push(kbd(*vk, false));
    }
    inputs.push(mouse_wheel(delta));
    for vk in mods.iter().rev() {
        inputs.push(kbd(*vk, true));
    }
    send(&inputs)
}

fn send_text(value: &str) -> Result<()> {
    let units: Vec<u16> = value.encode_utf16().collect();
    let mut start = 0;
    while start < units.len() {
        let mut end = (start + 64).min(units.len());
        // Never split a UTF-16 surrogate pair across two SendInput calls.
        if end < units.len() && (0xD800..=0xDBFF).contains(&units[end - 1]) {
            end -= 1;
        }
        let mut inputs = Vec::with_capacity((end - start) * 2);
        for unit in &units[start..end] {
            inputs.push(INPUT {
                r#type: INPUT_KEYBOARD,
                Anonymous: INPUT_0 {
                    ki: KEYBDINPUT {
                        wVk: 0,
                        wScan: *unit,
                        dwFlags: KEYEVENTF_UNICODE,
                        time: 0,
                        dwExtraInfo: MAGIC,
                    },
                },
            });
            inputs.push(INPUT {
                r#type: INPUT_KEYBOARD,
                Anonymous: INPUT_0 {
                    ki: KEYBDINPUT {
                        wVk: 0,
                        wScan: *unit,
                        dwFlags: KEYEVENTF_UNICODE | KEYEVENTF_KEYUP,
                        time: 0,
                        dwExtraInfo: MAGIC,
                    },
                },
            });
        }
        send(&inputs)?;
        start = end;
    }
    Ok(())
}

fn wide(value: &str) -> Vec<u16> {
    let mut v: Vec<u16> = std::ffi::OsStr::new(value).encode_wide().collect();
    v.push(0);
    v
}

fn open_target(target: &str, operation: &str) -> Result<()> {
    let op = wide(operation);
    let file = wide(target);
    let result = unsafe {
        ShellExecuteW(
            std::ptr::null_mut(),
            op.as_ptr(),
            file.as_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            SW_SHOWNORMAL,
        )
    };
    if result as isize <= 32 {
        bail!(
            "ShellExecute failed for {target:?} (code {})",
            result as isize
        );
    }
    Ok(())
}

fn window_action(what: &str) -> Result<()> {
    let hwnd = unsafe { GetForegroundWindow() };
    if hwnd.is_null() {
        bail!("no foreground window");
    }
    unsafe {
        match what.to_ascii_lowercase().as_str() {
            "minimize" | "min" => {
                ShowWindow(hwnd, SW_MINIMIZE);
            }
            "maximize" | "max" => {
                ShowWindow(hwnd, SW_MAXIMIZE);
            }
            "restore" => {
                ShowWindow(hwnd, SW_RESTORE);
            }
            "close" => {
                PostMessageW(hwnd, WM_CLOSE, 0, 0);
            }
            other => bail!("unknown window action {other:?}"),
        }
    }
    Ok(())
}

fn run_script(cmd: &str) -> Result<()> {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;

    std::process::Command::new("cmd")
        .args(["/C", cmd])
        .current_dir(std::env::current_dir().unwrap_or_else(|_| Path::new(".").to_path_buf()))
        .creation_flags(CREATE_NO_WINDOW)
        .spawn()?;
    Ok(())
}

pub fn execute(action: &Action) -> Result<()> {
    match action {
        Action::Hotkey { keys, .. } => send_hotkey(keys),
        Action::Sequence { steps } => {
            for step in steps {
                match step {
                    SequenceStep::Keys { keys } => send_hotkey(keys)?,
                    SequenceStep::Text { text } => send_text(text)?,
                    SequenceStep::Delay { delay } => {
                        std::thread::sleep(std::time::Duration::from_millis((*delay).min(2_000)))
                    }
                }
            }
            Ok(())
        }
        Action::Media { key } => send_media(key),
        Action::Wheel { delta, modifiers } => send_wheel(*delta, modifiers),
        Action::Text { value } => send_text(value),
        Action::Launch { target } | Action::Url { target } => open_target(target, "open"),
        Action::Script { cmd } => run_script(cmd),
        Action::Window { what } => window_action(what),
        Action::Layer { .. } | Action::Peek | Action::Nothing => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_names_cover_actions_and_reject_typos() {
        assert_eq!(key_to_vk("ctrl"), Some(0xA2));
        assert_eq!(key_to_vk("F13"), Some(0x7C));
        assert_eq!(key_to_vk("num0"), Some(0x60));
        assert_eq!(key_to_vk("volume_up"), Some(0xAF));
        assert_eq!(key_to_vk("pageup"), Some(0x21));
        assert_eq!(key_to_vk("definitely-not-a-key"), None);
    }
}
