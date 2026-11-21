use crate::Result;
use std::os::windows::ffi::OsStrExt;
use windows::{
    Win32::{Foundation::ERROR_FILE_NOT_FOUND, System::Registry::*},
    core::{PCWSTR, w},
};

const RUN: PCWSTR = w!("Software\\Microsoft\\Windows\\CurrentVersion\\Run");
const NAME: PCWSTR = w!("QuickVault");

pub fn enabled() -> Result<bool> {
    let mut bytes = 0;
    let result = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            RUN,
            NAME,
            RRF_RT_REG_SZ,
            None,
            None,
            Some(&mut bytes),
        )
    };
    if result == ERROR_FILE_NOT_FOUND {
        return Ok(false);
    }
    result.ok()?;
    Ok(true)
}
pub fn set(enabled: bool) -> Result<()> {
    unsafe {
        if enabled {
            let path = std::env::current_exe()?;
            let command: Vec<u16> = std::iter::once(34)
                .chain(path.as_os_str().encode_wide())
                .chain("\" --background\0".encode_utf16())
                .collect();
            RegSetKeyValueW(
                HKEY_CURRENT_USER,
                RUN,
                NAME,
                REG_SZ.0,
                Some(command.as_ptr().cast()),
                (command.len() * 2) as u32,
            )
            .ok()?;
        } else {
            let result = RegDeleteKeyValueW(HKEY_CURRENT_USER, RUN, NAME);
            if result != ERROR_FILE_NOT_FOUND {
                result.ok()?;
            }
        }
    }
    Ok(())
}
