#![windows_subsystem = "windows"]
mod clipboard;
mod database;
mod hotkey;
mod startup;
mod tray;
mod ui;

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

fn main() {
    use windows::Win32::UI::HiDpi::{
        DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, SetProcessDpiAwarenessContext,
    };
    unsafe {
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
    }
    let arg = std::env::args_os().nth(1);
    let result = match arg.as_deref().and_then(|s| s.to_str()) {
        None | Some("--pick") => tray::run(1),
        Some("--manage") => tray::run(2),
        Some("--settings") => tray::run(3),
        Some("--background") => tray::run(0),
        Some(_) => Err(
            "Use --pick, --manage or --settings, or launch without arguments for the tray.".into(),
        ),
    };
    if let Err(error) = result {
        use windows::{
            Win32::UI::WindowsAndMessaging::*,
            core::{HSTRING, w},
        };
        unsafe {
            MessageBoxW(
                None,
                &HSTRING::from(error.to_string()),
                w!("QuickVault"),
                MB_OK | MB_ICONERROR,
            );
        }
        std::process::exit(1);
    }
}
