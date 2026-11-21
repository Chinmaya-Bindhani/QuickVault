mod art;
mod paint;
use crate::{
    Result, clipboard,
    database::{Hit, Snippet, Vault, location},
    hotkey::{DEFAULT, Shortcut},
    startup, tray,
};
use std::{cell::RefCell, mem::size_of, os::windows::ffi::OsStringExt, path::PathBuf};
use windows::{
    Win32::{
        Foundation::*,
        Graphics::Gdi::*,
        System::LibraryLoader::GetModuleHandleW,
        UI::{
            Controls::{Dialogs::*, EM_SETLIMITTEXT, EM_SETSEL},
            HiDpi::*,
            Input::KeyboardAndMouse::*,
            Shell::{DefSubclassProc, SetWindowSubclass},
            WindowsAndMessaging::*,
        },
    },
    core::{HSTRING, PCWSTR, PWSTR, w},
};

const W: i32 = 704;
const H: i32 = 516;
const POPUP_WIDTH_DIP: i32 = 540;
const CLASS: PCWSTR = w!("QuickVault.Popup.2");
const EVENT: u32 = WM_APP + 70;
const SEARCH: u32 = 1;
const CLICK: u32 = 2;
const CONTEXT: u32 = 3;
const WHEEL: u32 = 4;
const CLOSE: u32 = 5;
const DPI: u32 = 6;
const HOVER: u32 = 7;
thread_local! {static WINDOW: RefCell<Option<App>> = const {RefCell::new(None)};}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Page {
    Search,
    Manage,
    Edit,
    Settings,
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum Target {
    None,
    Settings,
    Close,
    Search,
    Row(usize),
    Add,
    Manage,
    Save,
    Cancel,
    Delete,
    Pin,
    Start,
    AutoHide,
    Backup,
}

struct App {
    art: art::Artwork,
    hwnd: HWND,
    db: Vault,
    page: Page,
    back: Page,
    fields: Vec<HWND>,
    fonts: Vec<HFONT>,
    hits: Vec<Hit>,
    total: usize,
    selected: usize,
    offset: usize,
    query: String,
    edit: Option<(i64, i64)>,
    pinned: bool,
    hotkey: String,
    auto_hide: bool,
    start: bool,
    hover: Target,
    status: String,
    width: i32,
    dirty: bool,
}
impl Drop for App {
    fn drop(&mut self) {
        unsafe {
            if IsWindow(self.hwnd).as_bool() {
                let _ = DestroyWindow(self.hwnd);
            }
            for f in self.fonts.drain(..) {
                let _ = DeleteObject(f);
            }
        }
    }
}
fn field_text(hwnd: HWND) -> String {
    unsafe {
        let mut s = vec![0; GetWindowTextLengthW(hwnd) as usize + 1];
        let n = GetWindowTextW(hwnd, &mut s);
        String::from_utf16_lossy(&s[..n as usize])
    }
}
fn post(hwnd: HWND, action: u32, lp: LPARAM) {
    unsafe {
        let _ = PostMessageW(hwnd, EVENT, WPARAM(action as usize), lp);
    }
}
pub fn error(hwnd: HWND, s: &str) {
    unsafe {
        MessageBoxW(
            hwnd,
            &HSTRING::from(s),
            w!("QuickVault"),
            MB_ICONERROR | MB_OK,
        );
    }
}
fn ask(hwnd: HWND, s: &str) -> bool {
    unsafe {
        MessageBoxW(
            hwnd,
            &HSTRING::from(s),
            w!("QuickVault"),
            MB_ICONQUESTION | MB_YESNO | MB_DEFBUTTON2,
        ) == IDYES
    }
}
fn point(lp: LPARAM) -> (i32, i32) {
    (
        (lp.0 as u16 as i16) as i32,
        ((lp.0 >> 16) as u16 as i16) as i32,
    )
}
fn pressed(key: VIRTUAL_KEY) -> bool {
    unsafe { GetKeyState(key.0 as i32) < 0 }
}
fn redraw(hwnd: HWND) {
    unsafe {
        let _ = InvalidateRect(hwnd, None, false);
    }
}

unsafe extern "system" fn search_proc(
    hwnd: HWND,
    msg: u32,
    wp: WPARAM,
    lp: LPARAM,
    _: usize,
    _: usize,
) -> LRESULT {
    unsafe {
        let result = DefSubclassProc(hwnd, msg, wp, lp);
        if msg == WM_PAINT && GetWindowTextLengthW(hwnd) == 0 {
            let dc = GetDC(hwnd);
            let f = SendMessageW(hwnd, WM_GETFONT, WPARAM(0), LPARAM(0));
            let old = SelectObject(dc, HGDIOBJ(f.0 as *mut _));
            SetBkMode(dc, TRANSPARENT);
            SetTextColor(dc, paint::rgb(0xa9a9aa));
            let mut r = RECT::default();
            let _ = GetClientRect(hwnd, &mut r);
            r.left += 2;
            let mut cue: Vec<_> = "Search your snippets…".encode_utf16().collect();
            DrawTextW(
                dc,
                &mut cue,
                &mut r,
                DT_TOP | DT_SINGLELINE | DT_NOPREFIX | DT_END_ELLIPSIS,
            );
            SelectObject(dc, old);
            ReleaseDC(hwnd, dc);
        }
        result
    }
}

unsafe extern "system" fn proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    unsafe {
        match msg {
            WM_PAINT => {
                let mut ps = PAINTSTRUCT::default();
                let dc = BeginPaint(hwnd, &mut ps);
                WINDOW.with(|slot| {
                    if let Ok(state) = slot.try_borrow()
                        && let Some(app) = state.as_ref()
                    {
                        paint::render(hwnd, dc, app);
                    }
                });
                let _ = EndPaint(hwnd, &ps);
                LRESULT(0)
            }
            WM_ERASEBKGND => LRESULT(1),
            WM_CTLCOLOREDIT => {
                let dc = HDC(wp.0 as *mut _);
                SetTextColor(dc, paint::rgb(paint::INK));
                SetBkColor(dc, paint::rgb(paint::FIELD));
                SetDCBrushColor(dc, paint::rgb(paint::FIELD));
                LRESULT(GetStockObject(DC_BRUSH).0 as isize)
            }
            WM_COMMAND if (wp.0 >> 16) as u32 == EN_CHANGE => {
                post(hwnd, SEARCH, lp);
                LRESULT(0)
            }
            WM_LBUTTONDOWN => {
                let (x, y) = point(lp);
                let mut r = RECT::default();
                let _ = GetClientRect(hwnd, &mut r);
                if y < r.right * 65 / W && x < r.right * 585 / W {
                    let _ = ReleaseCapture();
                    SendMessageW(
                        hwnd,
                        WM_NCLBUTTONDOWN,
                        WPARAM(HTCAPTION as usize),
                        LPARAM(0),
                    );
                }
                LRESULT(0)
            }
            WM_LBUTTONUP => {
                post(hwnd, CLICK, lp);
                LRESULT(0)
            }
            WM_RBUTTONUP => {
                post(hwnd, CONTEXT, lp);
                LRESULT(0)
            }
            WM_MOUSEWHEEL => {
                post(hwnd, WHEEL, LPARAM(((wp.0 >> 16) as u16 as i16) as isize));
                LRESULT(0)
            }
            WM_MOUSEMOVE => {
                post(hwnd, HOVER, lp);
                LRESULT(0)
            }
            WM_CLOSE => {
                post(hwnd, CLOSE, LPARAM(0));
                LRESULT(0)
            }
            WM_DPICHANGED => {
                let r = *(lp.0 as *const RECT);
                let _ = SetWindowPos(
                    hwnd,
                    None,
                    r.left,
                    r.top,
                    r.right - r.left,
                    r.bottom - r.top,
                    SWP_NOZORDER | SWP_NOACTIVATE,
                );
                post(hwnd, DPI, LPARAM(0));
                LRESULT(0)
            }
            _ => DefWindowProcW(hwnd, msg, wp, lp),
        }
    }
}

impl App {
    fn new() -> Result<Self> {
        let mut db = Vault::open(&location()?)?;
        db.prepare_examples()?;
        let hotkey = db.setting("hotkey", DEFAULT)?;
        unsafe {
            let module = GetModuleHandleW(None)?;
            let wc = WNDCLASSW {
                style: CS_DROPSHADOW,
                lpfnWndProc: Some(proc),
                hInstance: module.into(),
                lpszClassName: CLASS,
                hCursor: LoadCursorW(None, IDC_ARROW)?,
                ..Default::default()
            };
            if RegisterClassW(&wc) == 0 && GetLastError() != ERROR_CLASS_ALREADY_EXISTS {
                return Err(windows::core::Error::from_win32().into());
            }
            let mut mouse = POINT::default();
            let _ = GetCursorPos(&mut mouse);
            let monitor = MonitorFromPoint(mouse, MONITOR_DEFAULTTONEAREST);
            let mut info = MONITORINFO {
                cbSize: size_of::<MONITORINFO>() as u32,
                ..Default::default()
            };
            GetMonitorInfoW(monitor, &mut info).ok()?;
            let (mut dx, mut dy) = (96, 96);
            let _ = GetDpiForMonitor(monitor, MDT_EFFECTIVE_DPI, &mut dx, &mut dy);
            let work = info.rcWork;
            let width = (POPUP_WIDTH_DIP * dx as i32 / 96)
                .min(work.right - work.left - 16)
                .min((work.bottom - work.top - 16) * W / H)
                .max(1);
            let height = width * H / W;
            let art = art::Artwork::load()?;
            let hwnd = CreateWindowExW(
                WS_EX_TOPMOST | WS_EX_TOOLWINDOW,
                CLASS,
                w!("QuickVault"),
                WS_POPUP | WS_CLIPCHILDREN,
                (work.left + work.right - width) / 2,
                (work.top + work.bottom - height) / 2,
                width,
                height,
                None,
                None,
                module,
                None,
            )?;
            let app = Self {
                art,
                hwnd,
                db,
                page: Page::Search,
                back: Page::Search,
                fields: vec![],
                fonts: vec![],
                hits: vec![],
                total: 0,
                selected: 0,
                offset: 0,
                query: String::new(),
                edit: None,
                pinned: false,
                hotkey,
                auto_hide: true,
                start: false,
                hover: Target::None,
                status: String::new(),
                width,
                dirty: false,
            };
            app.round_window();
            Ok(app)
        }
    }
    fn px(&self, n: i32) -> i32 {
        (n * self.width + W / 2) / W
    }
    fn round_window(&self) {
        unsafe {
            let r = CreateRoundRectRgn(
                0,
                0,
                self.width + 1,
                self.px(H) + 1,
                self.px(46),
                self.px(46),
            );
            if SetWindowRgn(self.hwnd, r, true) == 0 {
                let _ = DeleteObject(r);
            }
        }
    }
    fn clear(&mut self) {
        unsafe {
            for child in self.fields.drain(..) {
                let _ = DestroyWindow(child);
            }
            for f in self.fonts.drain(..) {
                let _ = DeleteObject(f);
            }
        }
        self.hover = Target::None;
        self.status.clear();
        self.dirty = false;
    }
    fn field(&mut self, rect: [i32; 4], value: &str, multi: bool, size: i32) -> Result<HWND> {
        let [x, y, width, height] = rect;
        unsafe {
            let style = if multi {
                ES_MULTILINE | ES_AUTOVSCROLL | ES_WANTRETURN
            } else {
                ES_AUTOHSCROLL
            };
            let mut ws = WS_CHILD | WS_VISIBLE | WS_TABSTOP | WINDOW_STYLE(style as u32);
            if multi {
                ws |= WS_VSCROLL;
            }
            let h = CreateWindowExW(
                WINDOW_EX_STYLE(0),
                w!("EDIT"),
                &HSTRING::from(value),
                ws,
                self.px(x),
                self.px(y),
                self.px(width),
                self.px(height),
                self.hwnd,
                HMENU((100 + self.fields.len()) as *mut _),
                GetModuleHandleW(None)?,
                None,
            )?;
            let f = paint::font(self.px(size), 400, w!("Segoe UI"));
            SendMessageW(h, WM_SETFONT, WPARAM(f.0 as usize), LPARAM(1));
            SendMessageW(h, EM_SETLIMITTEXT, WPARAM(0x7fff_fffe), LPARAM(0));
            self.fonts.push(f);
            self.fields.push(h);
            Ok(h)
        }
    }
    fn focus(&self) {
        if let Some(h) = self.fields.first() {
            unsafe {
                let _ = SetFocus(*h);
            }
        }
    }
    fn reload(&mut self) -> Result<()> {
        self.total = self.db.count()?;
        self.hits = self.db.search(&self.query, self.offset, 6)?;
        if self.offset > 0 && self.hits.is_empty() {
            self.offset = 0;
            self.hits = self.db.search(&self.query, 0, 6)?;
        }
        self.selected = self.selected.min(self.hits.len().min(5).saturating_sub(1));
        Ok(())
    }
    fn list(&mut self, page: Page) -> Result<()> {
        self.clear();
        self.page = page;
        self.back = page;
        self.edit = None;
        self.hotkey = self.db.setting("hotkey", DEFAULT)?;
        let chip = shortcut_width(&self.hotkey);
        let h = self.field([87, 83, 563 - chip, 31], &self.query.clone(), false, 21)?;
        unsafe {
            let _ = SetWindowSubclass(h, Some(search_proc), 1, 0);
            SendMessageW(h, EM_SETSEL, WPARAM(0), LPARAM(-1));
        }
        self.reload()?;
        self.focus();
        Ok(())
    }
    fn editor(&mut self, id: Option<i64>) -> Result<()> {
        let (item, revision) = if let Some(id) = id {
            self.db.get(id)?
        } else {
            (
                Snippet {
                    title: String::new(),
                    category: String::new(),
                    content: String::new(),
                    pinned: false,
                },
                0,
            )
        };
        self.clear();
        self.page = Page::Edit;
        self.edit = id.map(|id| (id, revision));
        self.pinned = item.pinned;
        self.field([44, 133, 613, 26], &item.title, false, 18)?;
        self.field([44, 204, 613, 26], &item.category, false, 18)?;
        self.field([44, 278, 613, 103], &item.content, true, 17)?;
        self.focus();
        Ok(())
    }
    fn settings(&mut self) -> Result<()> {
        let hot = self.db.setting("hotkey", DEFAULT)?;
        let hide = self.db.setting("auto_hide", "1")? == "1";
        let start = startup::enabled()?;
        self.clear();
        self.page = Page::Settings;
        self.auto_hide = hide;
        self.start = start;
        self.field([44, 169, 612, 27], &hot, false, 18)?;
        self.focus();
        Ok(())
    }
    fn save(&mut self) -> Result<()> {
        match self.page {
            Page::Edit => {
                let item = Snippet {
                    title: field_text(self.fields[0]),
                    category: field_text(self.fields[1]),
                    content: field_text(self.fields[2]),
                    pinned: self.pinned,
                };
                self.db.save(self.edit, &item)?;
                self.query.clear();
                self.offset = 0;
                self.selected = 0;
            }
            Page::Settings => {
                let key = Shortcut::parse(&field_text(self.fields[0]))?;
                // Each option is independent. Report a failed write and leave the form open for retry.
                tray::update_shortcut(key)?;
                if startup::enabled()? != self.start {
                    startup::set(self.start)?;
                }
                self.db
                    .set("auto_hide", if self.auto_hide { "1" } else { "0" })?;
            }
            _ => return Ok(()),
        }
        self.list(self.back)
    }
    fn backup(&self) -> Result<()> {
        let mut path = [0u16; 4096];
        let name: Vec<_> = "QuickVault backup.sqlite3".encode_utf16().collect();
        path[..name.len()].copy_from_slice(&name);
        let filter: Vec<_> = "SQLite backup\0*.sqlite3\0All files\0*.*\0\0"
            .encode_utf16()
            .collect();
        let mut dialog = OPENFILENAMEW {
            lStructSize: size_of::<OPENFILENAMEW>() as u32,
            hwndOwner: self.hwnd,
            lpstrFile: PWSTR(path.as_mut_ptr()),
            nMaxFile: path.len() as u32,
            lpstrFilter: PCWSTR(filter.as_ptr()),
            lpstrDefExt: w!("sqlite3"),
            lpstrTitle: w!("Back up QuickVault"),
            Flags: OFN_PATHMUSTEXIST | OFN_NOCHANGEDIR,
            ..Default::default()
        };
        unsafe {
            if !GetSaveFileNameW(&mut dialog).as_bool() {
                if CommDlgExtendedError().0 != 0 {
                    return Err("Windows could not open the backup dialog.".into());
                }
                return Ok(());
            }
        }
        let end = path.iter().position(|c| *c == 0).unwrap_or(path.len());
        self.db
            .backup(&PathBuf::from(std::ffi::OsString::from_wide(&path[..end])))?;
        unsafe {
            MessageBoxW(self.hwnd, w!("Backup saved."), w!("QuickVault"), MB_OK);
        }
        Ok(())
    }
    fn discard(&self) -> bool {
        !self.dirty || ask(self.hwnd, "Discard the unsaved changes?")
    }
    fn row_height(&self) -> i32 {
        if self.total == 4 { 76 } else { 63 }
    }
    fn hit(&self, x: i32, y: i32) -> Target {
        use Target::*;
        if (643..685).contains(&x) && (12..58).contains(&y) {
            return Close;
        }
        match self.page {
            Page::Search | Page::Manage => {
                if (589..640).contains(&x) && (12..58).contains(&y) {
                    return Settings;
                }
                if (21..683).contains(&x) && (67..130).contains(&y) {
                    return Search;
                }
                if (21..683).contains(&x) && (149..464).contains(&y) {
                    let i = ((y - 149) / self.row_height()) as usize;
                    if i < self.hits.len().min(5) {
                        return Row(i);
                    }
                }
                if (466..510).contains(&y) {
                    if x >= 604 {
                        return Add;
                    }
                    if (22..360).contains(&x) {
                        return Manage;
                    }
                }
            }
            Page::Edit | Page::Settings => {
                if (464..508).contains(&y) {
                    if (565..=676).contains(&x) {
                        return Save;
                    }
                    if (468..557).contains(&x) {
                        return Cancel;
                    }
                    if x < 115 && self.page == Page::Edit && self.edit.is_some() {
                        return Delete;
                    }
                }
                if (30..672).contains(&x) {
                    if self.page == Page::Edit && (395..436).contains(&y) {
                        return Pin;
                    }
                    if self.page == Page::Settings {
                        return match y {
                            232..=272 => AutoHide,
                            288..=330 => Start,
                            378..=424 if x < 170 => Backup,
                            _ => None,
                        };
                    }
                }
            }
        }
        None
    }
    fn move_selection(&mut self, down: bool) -> Result<()> {
        let count = self.hits.len().min(5);
        if down {
            if self.selected + 1 < count {
                self.selected += 1;
            } else if self.hits.len() > 5 {
                self.offset += 1;
                self.reload()?;
            }
        } else if self.selected > 0 {
            self.selected -= 1;
        } else if self.offset > 0 {
            self.offset -= 1;
            self.reload()?;
        }
        Ok(())
    }
    fn copy(&mut self, keep: bool) -> Result<bool> {
        if let Some(h) = self.hits.get(self.selected) {
            let item = self.db.get(h.id)?.0;
            clipboard::copy(self.hwnd, &item.content)?;
            if !keep && self.db.setting("auto_hide", "1")? == "1" {
                if let Some(tray) = tray::resident() {
                    unsafe {
                        let _ = PostMessageW(tray, tray::COPIED, WPARAM(0), LPARAM(0));
                    }
                }
                return Ok(true);
            }
            self.status = "Copied to clipboard".into();
            self.focus();
        }
        Ok(false)
    }
    fn delete(&mut self) -> Result<()> {
        if let Some((id, rev)) = self.edit
            && ask(self.hwnd, "Delete this snippet?")
        {
            self.db.delete(id, rev)?;
            self.list(self.back)?;
        }
        Ok(())
    }
    fn act(&mut self, t: Target, keep: bool) -> Result<bool> {
        match t {
            Target::Close => return Ok(self.discard()),
            Target::Cancel => {
                if self.discard() {
                    self.list(self.back)?;
                }
            }
            Target::Search => self.focus(),
            Target::Row(i) => {
                self.selected = i;
                if self.page == Page::Manage {
                    self.editor(Some(self.hits[i].id))?;
                } else {
                    return self.copy(keep);
                }
            }
            Target::Settings => self.settings()?,
            Target::Add => self.editor(None)?,
            Target::Manage => self.list(if self.page == Page::Manage {
                Page::Search
            } else {
                Page::Manage
            })?,
            Target::Save => self.save()?,
            Target::Delete => self.delete()?,
            Target::Pin => {
                self.pinned = !self.pinned;
                self.dirty = true;
            }
            Target::Start => {
                self.start = !self.start;
                self.dirty = true;
            }
            Target::AutoHide => {
                self.auto_hide = !self.auto_hide;
                self.dirty = true;
            }
            Target::Backup => self.backup()?,
            Target::None => {}
        }
        Ok(false)
    }
    fn context(&mut self, i: usize) -> Result<bool> {
        self.selected = i;
        unsafe {
            let h = CreatePopupMenu()?;
            let res = (|| -> Result<i32> {
                for (id, s) in [
                    (1, "Copy"),
                    (2, "Edit snippet"),
                    (3, if self.hits[i].pinned { "Unpin" } else { "Pin" }),
                    (4, "Delete…"),
                ] {
                    AppendMenuW(h, MF_STRING, id, &HSTRING::from(s))?;
                }
                let mut p = POINT::default();
                GetCursorPos(&mut p)?;
                Ok(TrackPopupMenu(
                    h,
                    TPM_RETURNCMD | TPM_RIGHTBUTTON,
                    p.x,
                    p.y,
                    0,
                    self.hwnd,
                    None,
                )
                .0)
            })();
            let _ = DestroyMenu(h);
            let id = self.hits[i].id;
            match res? {
                1 => return self.copy(false),
                2 => self.editor(Some(id))?,
                3 => {
                    let (mut item, rev) = self.db.get(id)?;
                    item.pinned = !item.pinned;
                    self.db.save(Some((id, rev)), &item)?;
                    self.reload()?;
                }
                4 => {
                    let (_, rev) = self.db.get(id)?;
                    if ask(self.hwnd, "Delete this snippet?") {
                        self.db.delete(id, rev)?;
                        self.reload()?;
                    }
                }
                _ => {}
            }
        }
        Ok(false)
    }
    fn event(&mut self, event: u32, lp: LPARAM) -> Result<bool> {
        match event {
            SEARCH => {
                if !self.fields.contains(&HWND(lp.0 as *mut _)) {
                    return Ok(false);
                }
                if matches!(self.page, Page::Search | Page::Manage) {
                    self.query = field_text(self.fields[0]);
                    self.offset = 0;
                    self.selected = 0;
                    self.status.clear();
                    self.reload()?;
                } else {
                    self.dirty = true;
                }
            }
            CLICK | HOVER | CONTEXT => {
                let (x, y) = point(lp);
                let t = self.hit(x * W / self.width, y * W / self.width);
                if event == CLICK {
                    let keep = matches!(t, Target::Row(i) if x*W/self.width>=571
                        && self.hits[i].category.eq_ignore_ascii_case("command"));
                    return self.act(t, keep);
                }
                if event == CONTEXT
                    && let Target::Row(i) = t
                {
                    return self.context(i);
                }
                if event == HOVER && t != self.hover {
                    self.hover = t;
                    redraw(self.hwnd);
                }
            }
            WHEEL if matches!(self.page, Page::Search | Page::Manage) => {
                if lp.0 < 0 && self.hits.len() > 5 {
                    self.offset += 1;
                } else if lp.0 > 0 {
                    self.offset = self.offset.saturating_sub(1);
                }
                self.reload()?;
            }
            CLOSE => return Ok(self.discard()),
            DPI => {
                let mut r = RECT::default();
                unsafe {
                    GetClientRect(self.hwnd, &mut r)?;
                }
                let old = self.width;
                self.width = r.right.max(1);
                self.round_window();
                // Rescale the live controls, retaining any unsaved text and caret positions.
                for (i, &h) in self.fields.iter().enumerate() {
                    unsafe {
                        let mut rc = RECT::default();
                        GetWindowRect(h, &mut rc)?;
                        let mut origin = POINT {
                            x: rc.left,
                            y: rc.top,
                        };
                        let _ = ScreenToClient(self.hwnd, &mut origin);
                        MoveWindow(
                            h,
                            origin.x * self.width / old,
                            origin.y * self.width / old,
                            (rc.right - rc.left) * self.width / old,
                            (rc.bottom - rc.top) * self.width / old,
                            true,
                        )?;
                        let size = match self.page {
                            Page::Search | Page::Manage => 21,
                            Page::Edit if i == 2 => 17,
                            _ => 18,
                        };
                        let f = paint::font(self.px(size), 400, w!("Segoe UI"));
                        SendMessageW(h, WM_SETFONT, WPARAM(f.0 as usize), LPARAM(1));
                        let _ = DeleteObject(self.fonts[i]);
                        self.fonts[i] = f;
                    }
                }
            }
            _ => {}
        }
        Ok(false)
    }
    fn key(&mut self, key: u32) -> Result<Option<bool>> {
        let list = matches!(self.page, Page::Search | Page::Manage);
        let ctrl = pressed(VK_CONTROL);
        if key == 27 {
            if list {
                return Ok(Some(true));
            }
            self.act(Target::Cancel, false)?;
            return Ok(Some(false));
        }
        if key == 9 {
            unsafe {
                let f = GetFocus();
                if let Ok(h) = GetNextDlgTabItem(self.hwnd, f, pressed(VK_SHIFT)) {
                    let _ = SetFocus(h);
                }
            }
        } else if list {
            match key {
                38 | 40 => self.move_selection(key == 40)?,
                13 => {
                    if self.page == Page::Manage && !ctrl {
                        if let Some(h) = self.hits.get(self.selected) {
                            self.editor(Some(h.id))?;
                        }
                    } else {
                        return Ok(Some(self.copy(ctrl)?));
                    }
                }
                113 => {
                    if let Some(h) = self.hits.get(self.selected) {
                        self.editor(Some(h.id))?;
                    }
                }
                78 if ctrl => self.editor(None)?,
                188 if ctrl => self.settings()?,
                _ => return Ok(None),
            }
        } else if key == 13
            && (ctrl
                || self.page == Page::Settings
                || unsafe { GetFocus() } != *self.fields.last().unwrap())
        {
            self.save()?;
        } else {
            return Ok(None);
        }
        Ok(Some(false))
    }
}

fn shortcut_width(label: &str) -> i32 {
    (label.replace('+', " + ").chars().count() as i32 * 8 + 18).clamp(74, 230)
}

pub fn open(mode: i32) -> Result<()> {
    WINDOW.with(|slot| -> Result<()> {
        let mut state = slot.borrow_mut();
        if state.is_none() {
            *state = Some(App::new()?);
        }
        let a = state.as_mut().unwrap();
        if !matches!(a.page, Page::Edit | Page::Settings) || a.discard() {
            a.query.clear();
            a.offset = 0;
            a.selected = 0;
            match mode {
                3 => a.settings()?,
                2 => a.list(Page::Manage)?,
                _ => a.list(Page::Search)?,
            }
        }
        let hwnd = a.hwnd;
        drop(state);
        unsafe {
            let _ = ShowWindow(hwnd, SW_SHOW);
            let _ = SetForegroundWindow(hwnd);
        }
        if let Some(a) = slot.borrow().as_ref() {
            a.focus();
        }
        redraw(hwnd);
        Ok(())
    })
}
pub fn is_open() -> bool {
    WINDOW.with(|slot| slot.borrow().is_some())
}
pub fn close() -> bool {
    WINDOW.with(|slot| {
        let Ok(mut state) = slot.try_borrow_mut() else {
            return false;
        };
        if state.as_ref().is_some_and(|a| !a.discard()) {
            return false;
        }
        state.take();
        true
    })
}
pub fn route(msg: &MSG) -> bool {
    if msg.message != EVENT && msg.message != WM_KEYDOWN {
        return false;
    }
    WINDOW.with(|slot| {
        let mut state = slot.borrow_mut();
        let Some(a) = state.as_mut() else {
            return false;
        };
        if msg.hwnd != a.hwnd && !unsafe { IsChild(a.hwnd, msg.hwnd) }.as_bool() {
            return false;
        }
        let result = if msg.message == EVENT {
            a.event(msg.wParam.0 as u32, msg.lParam).map(Some)
        } else {
            a.key(msg.wParam.0 as u32)
        };
        match result {
            Ok(Some(true)) => {
                state.take();
            }
            Ok(Some(false)) => {
                if msg.message != EVENT || msg.wParam.0 != HOVER as usize {
                    redraw(a.hwnd);
                }
            }
            Ok(None) => return false,
            Err(e) => {
                error(a.hwnd, &e.to_string());
                redraw(a.hwnd);
            }
        }
        true
    })
}
