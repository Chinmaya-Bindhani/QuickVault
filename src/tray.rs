use crate::{
    Result,
    database::{Vault, location},
    hotkey::{DEFAULT, Shortcut},
};
use std::{cell::Cell, mem::size_of};
use windows::{
    Win32::{
        Foundation::*,
        System::{LibraryLoader::GetModuleHandleW, Threading::CreateMutexW},
        UI::{
            Input::KeyboardAndMouse::{RegisterHotKey, UnregisterHotKey},
            Shell::*,
            WindowsAndMessaging::*,
        },
    },
    core::{HSTRING, PCWSTR, w},
};

const CLASS: PCWSTR = w!("QuickVault.Resident.1");
const TRAY: u32 = WM_APP + 1;
const OPEN: u32 = WM_APP + 2;
const REBIND: u32 = WM_APP + 3;
pub const COPIED: u32 = WM_APP + 4;
const ACTION: u32 = WM_APP + 5;
const SHOW_MENU: usize = 5;
const TOGGLE: usize = 6;
thread_local! {static TASKBAR:Cell<u32> = const { Cell::new(0) };}
thread_local! { static ACTIVE:Cell<Option<(Shortcut,i32)>> = const { Cell::new(None) }; }

pub fn resident() -> Option<HWND> {
    unsafe { FindWindowW(CLASS, None).ok() }
}
pub fn update_shortcut(key: Shortcut) -> Result<()> {
    let hwnd = resident()
        .ok_or("Start QuickVault from its executable before changing the global shortcut.")?;
    let mut result = 0;
    let sent = unsafe {
        SendMessageTimeoutW(
            hwnd,
            REBIND,
            WPARAM(key.mods as usize),
            LPARAM(key.key as isize),
            SMTO_ABORTIFHUNG,
            5000,
            Some(&mut result),
        )
    };
    if sent.0 == 0 {
        return Err(
            "The tray did not respond. Reopen Settings to check the active shortcut.".into(),
        );
    }
    match result {
        1 => Ok(()),
        2 => Err("Windows could not register this shortcut. Choose another combination.".into()),
        _ => Err("The shortcut could not be saved. Check the local database.".into()),
    }
}

fn bind(hwnd: HWND, key: Shortcut, persist: bool) -> usize {
    let old = ACTIVE.get();
    if old.is_some_and(|(active, _)| active == key) {
        return 1;
    }
    let id = if old.is_some_and(|(_, id)| id == 1) {
        2
    } else {
        1
    };
    unsafe {
        if RegisterHotKey(hwnd, id, key.flags(), key.key).is_err() {
            return 2;
        }
        if persist
            && location()
                .and_then(|p| Vault::open(&p))
                .and_then(|db| db.set("hotkey", &key.label()))
                .is_err()
        {
            let _ = UnregisterHotKey(hwnd, id);
            return 0;
        }
        if let Some((_, previous)) = old {
            let _ = UnregisterHotKey(hwnd, previous);
        }
    }
    ACTIVE.set(Some((key, id)));
    1
}

unsafe extern "system" fn proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    unsafe {
        match msg {
            _ if msg != 0 && msg == TASKBAR.get() => {
                let _ = icon_add(hwnd);
                LRESULT(0)
            }
            TRAY | OPEN | WM_HOTKEY => {
                let action = match msg {
                    OPEN => 1,
                    WM_HOTKEY if ACTIVE.get().is_some_and(|(_, id)| wp.0 == id as usize) => TOGGLE,
                    TRAY if lp.0 == WM_LBUTTONUP as isize => 1,
                    TRAY if lp.0 == WM_RBUTTONUP as isize => SHOW_MENU,
                    _ => 0,
                };
                if action != 0 {
                    let _ = PostMessageW(hwnd, ACTION, WPARAM(action), LPARAM(0));
                }
                LRESULT(0)
            }
            COPIED => {
                notify(hwnd, "Copied to clipboard.");
                LRESULT(0)
            }
            REBIND => LRESULT(
                Shortcut::from_parts(wp.0 as u32, lp.0 as u32)
                    .map(|key| bind(hwnd, key, true) as isize)
                    .unwrap_or(0),
            ),
            WM_CLOSE => {
                if crate::ui::close() {
                    let _ = DestroyWindow(hwnd);
                }
                LRESULT(0)
            }
            WM_DESTROY => {
                if let Some((_, id)) = ACTIVE.take() {
                    let _ = UnregisterHotKey(hwnd, id);
                }
                icon_remove(hwnd);
                PostQuitMessage(0);
                LRESULT(0)
            }
            _ => DefWindowProcW(hwnd, msg, wp, lp),
        }
    }
}

fn icon_data(hwnd: HWND) -> NOTIFYICONDATAW {
    NOTIFYICONDATAW {
        cbSize: size_of::<NOTIFYICONDATAW>() as u32,
        hWnd: hwnd,
        uID: 1,
        ..Default::default()
    }
}
fn fill(out: &mut [u16], s: &str) {
    let length = out.len() - 1;
    for (target, c) in out[..length].iter_mut().zip(s.encode_utf16()) {
        *target = c;
    }
}
fn icon_add(hwnd: HWND) -> Result<()> {
    let mut data = icon_data(hwnd);
    data.uFlags = NIF_ICON | NIF_MESSAGE | NIF_TIP;
    data.uCallbackMessage = TRAY;
    // Win32 encodes integer resource IDs in the low word of a PCWSTR.
    data.hIcon = unsafe {
        LoadIconW(
            GetModuleHandleW(None)?,
            PCWSTR(std::ptr::without_provenance(1)),
        )?
    };
    fill(&mut data.szTip, "QuickVault");
    unsafe { Shell_NotifyIconW(NIM_ADD, &data).ok()? };
    Ok(())
}
fn icon_remove(hwnd: HWND) {
    unsafe {
        let _ = Shell_NotifyIconW(NIM_DELETE, &icon_data(hwnd));
    }
}
fn notify(hwnd: HWND, text: &str) {
    let mut data = icon_data(hwnd);
    data.uFlags = NIF_INFO;
    data.dwInfoFlags = NIIF_INFO;
    fill(&mut data.szInfoTitle, "QuickVault");
    fill(&mut data.szInfo, text);
    unsafe {
        let _ = Shell_NotifyIconW(NIM_MODIFY, &data);
    }
}
fn menu(hwnd: HWND) -> Result<i32> {
    unsafe {
        let h = CreatePopupMenu()?;
        let result = (|| -> Result<i32> {
            for (id, label) in [
                (1, "Open QuickVault"),
                (2, "Manage Snippets"),
                (3, "Settings"),
                (4, "Exit QuickVault"),
            ] {
                AppendMenuW(h, MF_STRING, id, &HSTRING::from(label))?;
            }
            let mut p = POINT::default();
            GetCursorPos(&mut p)?;
            let _ = SetForegroundWindow(hwnd);
            let id = TrackPopupMenu(h, TPM_RETURNCMD | TPM_RIGHTBUTTON, p.x, p.y, 0, hwnd, None).0;
            let _ = PostMessageW(hwnd, WM_NULL, WPARAM(0), LPARAM(0));
            Ok(id)
        })();
        let _ = DestroyMenu(h);
        result
    }
}
struct Instance(HANDLE);
impl Drop for Instance {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}
pub fn run(initial: usize) -> Result<()> {
    unsafe {
        let _mutex = Instance(CreateMutexW(
            None,
            false,
            w!("Local\\QuickVault.Resident.1"),
        )?);
        if GetLastError() == ERROR_ALREADY_EXISTS {
            if let Some(hwnd) = resident()
                && initial != 0
            {
                let mut pid = 0;
                GetWindowThreadProcessId(hwnd, Some(&mut pid));
                let _ = AllowSetForegroundWindow(pid);
                PostMessageW(hwnd, ACTION, WPARAM(initial), LPARAM(0))?;
            }
            return Ok(());
        }
        let module = GetModuleHandleW(None)?;
        let class = WNDCLASSW {
            lpfnWndProc: Some(proc),
            hInstance: module.into(),
            lpszClassName: CLASS,
            ..Default::default()
        };
        if RegisterClassW(&class) == 0 {
            return Err(windows::core::Error::from_win32().into());
        }
        let hwnd = CreateWindowExW(
            WS_EX_TOOLWINDOW,
            CLASS,
            w!("QuickVault"),
            WINDOW_STYLE(0),
            0,
            0,
            0,
            0,
            None,
            None,
            module,
            None,
        )?;
        icon_add(hwnd)?;
        let key = location()
            .and_then(|p| Vault::open(&p))
            .and_then(|db| db.setting("hotkey", DEFAULT))
            .and_then(|s| Shortcut::parse(&s));
        match key {
            Ok(k) if bind(hwnd, k, false) == 2 => notify(
                hwnd,
                "The shortcut is unavailable. Open tray Settings to choose another.",
            ),
            Err(_) => notify(
                hwnd,
                "Could not load the vault or shortcut. Check the local database and tray Settings.",
            ),
            _ => {}
        }
        TASKBAR.set(RegisterWindowMessageW(w!("TaskbarCreated")));
        if initial != 0 {
            PostMessageW(hwnd, ACTION, WPARAM(initial), LPARAM(0))?;
        }
        let mut msg = MSG::default();
        loop {
            let got = GetMessageW(&mut msg, None, 0, 0).0;
            if got < 0 {
                return Err(windows::core::Error::from_win32().into());
            }
            if got == 0 {
                break;
            }
            if crate::ui::route(&msg) {
                continue;
            }
            let action = match msg.message {
                ACTION if msg.wParam.0 == TOGGLE => {
                    if crate::ui::is_open() {
                        crate::ui::close();
                        0
                    } else {
                        1
                    }
                }
                ACTION if msg.wParam.0 == SHOW_MENU => menu(hwnd).unwrap_or(0),
                ACTION => msg.wParam.0 as i32,
                _ => {
                    let _ = TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                    0
                }
            };
            if action == 4 && crate::ui::close() {
                DestroyWindow(hwnd)?;
            }
            if (1..=3).contains(&action)
                && let Err(e) = crate::ui::open(action)
            {
                crate::ui::error(hwnd, &e.to_string());
            }
        }
        Ok(())
    }
}
