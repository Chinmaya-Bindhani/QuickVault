use super::art::Artwork;
use super::{App, H, Page, W};
use windows::{
    Win32::{Foundation::*, Graphics::Gdi::*},
    core::{PCWSTR, w},
};

pub const INK: u32 = 0xf6f6f5;
const MUTED: u32 = 0xb2b2b2;
const EDGE: u32 = 0x535354;
pub const FIELD: u32 = 0x19191a;
const SILVER: u32 = 0xadadae;

pub fn rgb(c: u32) -> COLORREF {
    COLORREF((c >> 16) | (c & 0xff00) | ((c & 255) << 16))
}
pub fn font(px: i32, weight: i32, face: PCWSTR) -> HFONT {
    unsafe {
        CreateFontW(
            -px,
            0,
            0,
            0,
            weight,
            0,
            0,
            0,
            1,
            0,
            0,
            CLEARTYPE_QUALITY.0 as u32,
            0,
            face,
        )
    }
}
struct Canvas<'a>(HDC, &'a Artwork, i32);
impl Canvas<'_> {
    fn px(&self, n: i32) -> i32 {
        (n * self.2 + W / 2) / W
    }
    fn surface(&self, rect: [i32; 4], radius: i32, colors: [u32; 4], diagonal: bool) {
        let scale = self.2 as f32 / W as f32;
        self.1.surface(
            self.0,
            rect.map(|n| n as f32 * scale),
            radius as f32 * scale,
            colors,
            diagonal,
        );
    }
    fn round(&self, x: i32, y: i32, r: i32, b: i32, radius: i32, color: u32) {
        self.surface([x, y, r, b], radius, [color; 4], false);
    }
    fn glass(&self, rect: [i32; 4], radius: i32, rim: [u32; 4], face: [u32; 4]) {
        let [x, y, r, b] = rect;
        self.surface(rect, radius, rim, true);
        self.surface([x + 1, y + 1, r - 1, b - 1], radius - 1, face, false);
    }
    fn panel(&self) {
        self.glass(
            [0, 0, W, H],
            23,
            [0x979797, 0x525252, 0x282828, 0x6a6a6a],
            [0x242424, 0x101011, 0x0b0b0c, 0x151515],
        );
    }
    fn line(&self, points: &[(i32, i32)], width: i32, c: u32) {
        let pts: Vec<_> = points
            .iter()
            .map(|(x, y)| POINT {
                x: self.px(*x),
                y: self.px(*y),
            })
            .collect();
        unsafe {
            let pen = CreatePen(PS_SOLID, self.px(width).max(1), rgb(c));
            let old = SelectObject(self.0, pen);
            let _ = Polyline(self.0, &pts);
            SelectObject(self.0, old);
            let _ = DeleteObject(pen);
        }
    }
    #[allow(clippy::too_many_arguments)]
    fn label(
        &self,
        x: i32,
        y: i32,
        r: i32,
        b: i32,
        s: &str,
        size: i32,
        weight: i32,
        c: u32,
    ) -> i32 {
        self.text([x, y, r, b], s, size, weight, c, w!("Segoe UI"))
    }
    fn text(&self, rect: [i32; 4], s: &str, size: i32, weight: i32, c: u32, face: PCWSTR) -> i32 {
        let [x, y, r, b] = rect;
        let mut chars: Vec<_> = s.encode_utf16().collect();
        unsafe {
            let f = font(self.px(size), weight, face);
            let old = SelectObject(self.0, f);
            let _ = SetBkMode(self.0, TRANSPARENT);
            SetTextColor(self.0, rgb(c));
            let mut area = RECT {
                left: self.px(x),
                top: self.px(y),
                right: self.px(r),
                bottom: self.px(b),
            };
            let mut extent = SIZE::default();
            let _ = GetTextExtentPoint32W(self.0, &chars, &mut extent);
            DrawTextW(
                self.0,
                &mut chars,
                &mut area,
                DT_SINGLELINE | DT_VCENTER | DT_END_ELLIPSIS | DT_NOPREFIX,
            );
            SelectObject(self.0, old);
            let _ = DeleteObject(f);
            (extent.cx * W + self.2 / 2) / self.2
        }
    }
    fn glyph(&self, x: i32, y: i32, size: i32, s: &str, color: u32) {
        self.text(
            [x, y, x + size + 5, y + size + 5],
            s,
            size,
            400,
            color,
            w!("Segoe MDL2 Assets"),
        );
    }
    fn icon(&self, x: i32, y: i32, title: &str, category: &str, preview: &str) {
        let t = title.to_ascii_lowercase();
        let p = preview.to_ascii_lowercase();
        let (symbol, face, color, em) = if t.contains("github") || p.contains("github.com/") {
            ("\u{f09b}", w!("QuickVault Brands"), 0xf3f3f3, 34)
        } else if t.contains("linkedin") || p.contains("linkedin.com/") {
            ("\u{f08c}", w!("QuickVault Brands"), 0x6db6ec, 33)
        } else if t.contains("email") || p.contains('@') {
            ("\u{e715}", w!("Segoe MDL2 Assets"), 0x9ed9f0, 25)
        } else if t.contains("command") || category.eq_ignore_ascii_case("command") {
            ("\u{e943}", w!("Segoe MDL2 Assets"), 0xa3d6b4, 25)
        } else if p.starts_with("http") || t.contains("portfolio") {
            ("\u{e774}", w!("Segoe MDL2 Assets"), 0xc4bade, 27)
        } else {
            ("\u{e8a5}", w!("Segoe MDL2 Assets"), 0xd2d2d2, 24)
        };
        self.glass(
            [x - 1, y - 1, x + 41, y + 41],
            10,
            [0x5c5c5c, 0x404040, 0x282828, 0x464646],
            [0x343435, 0x29292a, 0x212122, 0x252526],
        );
        let inset = (40 - em) / 2;
        self.text([x + inset, y, x + 43, y + 40], symbol, em, 400, color, face);
    }
    fn header(&self, app: &App) {
        self.label(31, 13, 315, 55, "QuickVault", 24, 600, INK);
        if app.page == Page::Search || app.page == Page::Manage {
            if app.hover == super::Target::Settings {
                self.round(594, 16, 634, 54, 9, FIELD);
            }
            self.glyph(601, 21, 23, "\u{e713}", MUTED);
        }
        if app.hover == super::Target::Close {
            self.round(644, 16, 681, 54, 9, 0x382c2d);
        }
        self.line(&[(655, 27), (670, 42)], 2, MUTED);
        self.line(&[(670, 27), (655, 42)], 2, MUTED);
    }
    fn chip(&self, x: i32, y: i32, r: i32, b: i32, s: &str, accent: bool) {
        let face = if accent {
            [0x585859, 0x414142, 0x323233, 0x404041]
        } else {
            [0x333334, 0x29292a, 0x202021, 0x272728]
        };
        self.glass(
            [x, y, r, b],
            9,
            [0x656566, 0x434344, 0x2a2a2a, 0x4a4a4a],
            face,
        );
        self.label(
            x + 10,
            y,
            r - 5,
            b,
            s,
            17,
            500,
            if accent { INK } else { 0xd6d6d6 },
        );
    }
    fn results(&self, a: &App) {
        self.glass(
            [21, 67, 683, 129],
            31,
            [0x787879, 0x434343, 0x272727, 0x515151],
            [FIELD; 4],
        );
        self.glyph(44, 84, 25, "\u{e721}", 0xc2c2c2);
        let hot = a.hotkey.replace('+', " + ");
        let chip = super::shortcut_width(&a.hotkey);
        self.chip(664 - chip, 81, 664, 115, &hot, false);
        for (i, h) in a.hits.iter().take(5).enumerate() {
            let height = a.row_height();
            let top = 149 + i as i32 * height;
            let y = top + (height - 63) / 2;
            let active = i == a.selected;
            if active {
                self.glass(
                    [21, top, 683, top + height - 2],
                    13,
                    [0x9b9b9b, 0x5d5d5d, 0x363636, 0x666666],
                    [0x3a3a3b, 0x2c2c2d, 0x242425, 0x29292a],
                );
            } else if a.hover == super::Target::Row(i) {
                self.round(22, top + 1, 682, top + height - 3, 12, 0x202021);
            }
            self.icon(38, y + 12, &h.title, &h.category, &h.preview);
            let right = if a.page == Page::Manage { 554 } else { 590 };
            self.label(106, y + 6, right, y + 39, &h.title, 18, 600, INK);
            let preview = h.preview.replace(['\r', '\n', '\t'], " ");
            self.label(106, y + 32, right, y + 60, &preview, 17, 400, MUTED);
            if h.pinned {
                self.glyph(88, y + 23, 11, "\u{e718}", 0xcfcfcf);
            }
            if a.page == Page::Manage {
                self.chip(595, y + 14, 661, y + 49, "Edit", false);
            } else if h.category.eq_ignore_ascii_case("command") {
                self.chip(571, y + 14, 672, y + 48, "Ctrl + Enter", false);
            } else {
                if active {
                    self.round(592, y + 13, 632, y + 51, 9, 0x414142);
                }
                self.glyph(602, y + 20, 20, "\u{e8c8}", 0xe0e0e0);
                self.label(649, y + 13, 675, y + 50, "↵", 22, 400, MUTED);
            }
        }
        if a.hits.is_empty() {
            let msg = if a.total == 0 {
                "Your vault is empty"
            } else {
                "No matching snippets"
            };
            self.label(106, 178, 620, 216, msg, 21, 600, INK);
            self.label(
                106,
                215,
                650,
                248,
                if a.total == 0 {
                    "Use + Add to save your first snippet."
                } else {
                    "Try a different title or a word from the content."
                },
                16,
                400,
                MUTED,
            );
        }
        self.line(&[(22, 464), (682, 464)], 1, 0x303031);
        let foot = if !a.status.is_empty() {
            a.status.clone()
        } else if a.page == Page::Manage {
            "Manage Snippets".into()
        } else {
            "All Snippets".into()
        };
        self.glyph(32, 479, 18, "\u{e80a}", MUTED);
        let tw = self.label(62, 471, 445, 507, &foot, 17, 500, 0xdddddd);
        if a.status.is_empty() {
            self.label(
                71 + tw,
                471,
                507,
                507,
                &format!("({})", a.total),
                16,
                400,
                0x9a9a9b,
            );
        }
        if a.offset > 0 || a.hits.len() > 5 {
            self.label(
                511,
                471,
                605,
                507,
                &format!("{}–{}", a.offset + 1, a.offset + a.hits.len().min(5)),
                14,
                400,
                MUTED,
            );
        }
        self.chip(606, 475, 681, 505, "＋ Add", a.hover == super::Target::Add);
    }
    fn field(&self, y: i32, b: i32) {
        self.round(32, y, 672, b, 8, EDGE);
        self.round(33, y + 1, 671, b - 1, 7, FIELD);
    }
    fn toggle(&self, y: i32, label: &str, on: bool) {
        self.round(
            34,
            y + 4,
            67,
            y + 23,
            10,
            if on { SILVER } else { 0x444445 },
        );
        let left = if on { 49 } else { 37 };
        self.round(left, y + 7, left + 13, y + 20, 7, 0xf4f4f4);
        self.label(81, y - 1, 615, y + 28, label, 16, 500, INK);
    }
    fn form(&self, a: &App) {
        match a.page {
            Page::Edit => {
                self.label(
                    32,
                    63,
                    640,
                    101,
                    if a.edit.is_some() {
                        "Edit snippet"
                    } else {
                        "Add snippet"
                    },
                    25,
                    600,
                    INK,
                );
                for (label, top, bottom) in [
                    ("Title", 126, 165),
                    ("Category", 197, 236),
                    ("Content", 269, 389),
                ] {
                    self.label(33, top - 23, 550, top, label, 15, 500, MUTED);
                    self.field(top, bottom);
                }
                self.toggle(401, "Pin this snippet", a.pinned);
                if a.edit.is_some() {
                    self.chip(32, 469, 108, 502, "Delete", false);
                }
            }
            Page::Settings => {
                self.label(32, 63, 640, 104, "Settings", 25, 600, INK);
                self.label(33, 124, 600, 152, "Global keyboard shortcut", 17, 500, INK);
                self.field(159, 202);
                self.toggle(239, "Close after copying", a.auto_hide);
                self.toggle(296, "Start QuickVault when I sign in", a.start);
                self.line(&[(33, 358), (671, 358)], 1, EDGE);
                self.chip(33, 381, 167, 417, "Back up vault", false);
            }
            _ => {}
        }
        self.line(&[(22, 454), (682, 454)], 1, EDGE);
        self.chip(473, 469, 552, 502, "Cancel", false);
        self.chip(568, 469, 672, 502, "Save", true);
    }
}

pub fn render(hwnd: HWND, dc: HDC, app: &App) {
    unsafe {
        let mut size = RECT::default();
        if windows::Win32::UI::WindowsAndMessaging::GetClientRect(hwnd, &mut size).is_err() {
            return;
        }
        let mem = CreateCompatibleDC(dc);
        if mem.is_invalid() {
            return;
        }
        let bmp = CreateCompatibleBitmap(dc, size.right, size.bottom);
        if bmp.is_invalid() {
            let _ = DeleteDC(mem);
            return;
        }
        let old = SelectObject(mem, bmp);
        let c = Canvas(mem, &app.art, size.right);
        let _ = PatBlt(mem, 0, 0, size.right, size.bottom, BLACKNESS);
        c.panel();
        c.header(app);
        if matches!(app.page, Page::Search | Page::Manage) {
            c.results(app);
        } else {
            c.form(app);
        }
        let _ = BitBlt(dc, 0, 0, size.right, size.bottom, mem, 0, 0, SRCCOPY);
        SelectObject(mem, old);
        let _ = DeleteObject(bmp);
        let _ = DeleteDC(mem);
    }
}
