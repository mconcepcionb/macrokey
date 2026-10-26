//! Small append-only file logger.
//!
//! A release build is a GUI-subsystem binary, so stdout is invisible. Every
//! important message is mirrored to `agent.log` next to the database, which is
//! what users can attach to a bug report. Debug builds keep printing to the
//! console as well.

use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use windows_sys::Win32::Foundation::SYSTEMTIME;
use windows_sys::Win32::System::SystemInformation::GetLocalTime;

static LOG: OnceLock<Mutex<Option<File>>> = OnceLock::new();
static LOG_PATH: OnceLock<PathBuf> = OnceLock::new();

pub fn init(path: &Path) {
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    // Keep the log bounded without pulling in a logging framework. If another
    // process has the file open the rename fails harmlessly and we append.
    if std::fs::metadata(path)
        .map(|meta| meta.len() > 2_000_000)
        .unwrap_or(false)
    {
        let _ = std::fs::rename(path, path.with_extension("log.1"));
    }
    let file = OpenOptions::new().create(true).append(true).open(path).ok();
    let _ = LOG.set(Mutex::new(file));
    let _ = LOG_PATH.set(path.to_path_buf());
}

pub fn path() -> Option<&'static Path> {
    LOG_PATH.get().map(PathBuf::as_path)
}

pub fn write(error: bool, args: std::fmt::Arguments<'_>) {
    let line = format!("{} {}", timestamp(), args);

    if let Some(log) = LOG.get() {
        if let Ok(mut guard) = log.lock() {
            if let Some(file) = guard.as_mut() {
                let _ = writeln!(file, "{line}");
                // Flush every line so a crash still leaves the last event.
                let _ = file.flush();
            }
        }
    }

    if cfg!(debug_assertions) {
        if error {
            eprintln!("{line}");
        } else {
            println!("{line}");
        }
    }
}

fn timestamp() -> String {
    let mut time: SYSTEMTIME = unsafe { std::mem::zeroed() };
    unsafe { GetLocalTime(&mut time) };
    format!(
        "{:04}-{:02}-{:02} {:02}:{:02}:{:02}.{:03}",
        time.wYear,
        time.wMonth,
        time.wDay,
        time.wHour,
        time.wMinute,
        time.wSecond,
        time.wMilliseconds
    )
}

#[macro_export]
macro_rules! log_info {
    ($($arg:tt)*) => {
        $crate::logging::write(false, format_args!($($arg)*))
    };
}

#[macro_export]
macro_rules! log_error {
    ($($arg:tt)*) => {
        $crate::logging::write(true, format_args!($($arg)*))
    };
}
