use crate::Result;
use windows::Win32::{
    Foundation::{GlobalFree, HANDLE, HGLOBAL, HWND},
    System::{DataExchange::*, Memory::*},
};
use windows::core::w;

struct Block(HGLOBAL);
impl Block {
    fn unicode(text: &str) -> Result<Self> {
        let units = text.encode_utf16().chain(Some(0));
        unsafe {
            let block = Self(GlobalAlloc(
                GMEM_MOVEABLE,
                (text.encode_utf16().count() + 1) * 2,
            )?);
            let ptr = GlobalLock(block.0).cast::<u16>();
            if ptr.is_null() {
                return Err("Windows could not allocate clipboard text.".into());
            }
            for (i, unit) in units.enumerate() {
                ptr.add(i).write(unit);
            }
            let _ = GlobalUnlock(block.0);
            Ok(block)
        }
    }
    fn give(self, format: u32) -> Result<()> {
        unsafe {
            SetClipboardData(format, HANDLE(self.0.0))?;
        }
        std::mem::forget(self); // Windows owns this HGLOBAL after a successful transfer.
        Ok(())
    }
}
impl Drop for Block {
    fn drop(&mut self) {
        unsafe {
            let _ = GlobalFree(self.0);
        }
    }
}
struct Clipboard;
impl Drop for Clipboard {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseClipboard();
        }
    }
}

pub fn copy(owner: HWND, value: &str) -> Result<()> {
    unsafe {
        let format = RegisterClipboardFormatW(w!("ExcludeClipboardContentFromMonitorProcessing"));
        if format == 0 {
            return Err(windows::core::Error::from_win32().into());
        }
        let block = Block::unicode(value)?;
        let marker = Block(GlobalAlloc(GMEM_MOVEABLE | GMEM_ZEROINIT, 4)?);
        OpenClipboard(owner).map_err(|_| "The clipboard is busy. Try copying again.")?;
        let _close = Clipboard;
        EmptyClipboard()?;
        marker.give(format)?;
        block.give(13)?; // CF_UNICODETEXT
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn windows_text_block_preserves_exact_utf16() -> Result<()> {
        let text = "  Café 🦀\r\nsecond line\t ";
        let expected: Vec<_> = text.encode_utf16().chain(Some(0)).collect();
        let block = Block::unicode(text)?;
        unsafe {
            let ptr = GlobalLock(block.0).cast::<u16>();
            assert!(!ptr.is_null());
            assert_eq!(std::slice::from_raw_parts(ptr, expected.len()), expected);
            let _ = GlobalUnlock(block.0);
        }
        Ok(())
    }
}
