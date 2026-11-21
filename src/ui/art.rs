use crate::Result;
use std::ptr::null_mut;
use windows::Win32::{
    Foundation::HANDLE,
    Graphics::{
        Gdi::{AddFontMemResourceEx, HDC, RemoveFontMemResourceEx},
        GdiPlus as gp,
    },
};

const BRANDS: &[u8] = include_bytes!("../../assets/brands.ttf");

/// Native graphics resources live only as long as the popup.
pub(super) struct Artwork {
    token: usize,
    brands: HANDLE,
}
impl Artwork {
    pub(super) fn load() -> Result<Self> {
        unsafe {
            let input = gp::GdiplusStartupInput {
                GdiplusVersion: 1,
                SuppressExternalCodecs: true.into(),
                ..Default::default()
            };
            let mut token = 0;
            if gp::GdiplusStartup(&mut token, &input, null_mut()) != gp::Ok {
                return Err("Windows could not initialize the popup graphics.".into());
            }
            let mut count = 0;
            let brands = AddFontMemResourceEx(
                BRANDS.as_ptr().cast(),
                BRANDS.len() as u32,
                None,
                &raw mut count,
            );
            if brands.is_invalid() {
                gp::GdiplusShutdown(token);
                return Err("Windows could not load the snippet icons.".into());
            }
            Ok(Self { token, brands })
        }
    }

    /// Opaque, antialiased surfaces with a continuous specular gradient across the edge.
    pub(super) fn surface(
        &self,
        dc: HDC,
        rect: [f32; 4],
        radius: f32,
        colors: [u32; 4],
        diagonal: bool,
    ) {
        let [l, t, r, b] = rect;
        let d = (2.0 * radius).min(r - l).min(b - t);
        let from = gp::PointF { X: l, Y: t };
        let to = gp::PointF {
            X: if diagonal { r } else { l },
            Y: b,
        };
        let colors = colors.map(|rgb| 0xff000000 | rgb);
        let stops = [0.0, 0.28, 0.71, 1.0];
        unsafe {
            let (mut graphics, mut path, mut brush) = (null_mut(), null_mut(), null_mut());
            if gp::GdipCreateFromHDC(dc, &mut graphics) != gp::Ok {
                return;
            }
            gp::GdipSetSmoothingMode(graphics, gp::SmoothingModeAntiAlias8x8);
            if gp::GdipCreatePath(gp::FillModeWinding, &mut path) == gp::Ok {
                for (x, y, a) in [
                    (l, t, 180.),
                    (r - d, t, 270.),
                    (r - d, b - d, 0.),
                    (l, b - d, 90.),
                ] {
                    gp::GdipAddPathArc(path, x, y, d, d, a, 90.);
                }
                gp::GdipClosePathFigure(path);
                if gp::GdipCreateLineBrush(
                    &from,
                    &to,
                    colors[0],
                    colors[3],
                    gp::WrapModeTileFlipXY,
                    &mut brush,
                ) == gp::Ok
                {
                    gp::GdipSetLinePresetBlend(brush, colors.as_ptr(), stops.as_ptr(), 4);
                    gp::GdipFillPath(graphics, brush.cast(), path);
                    gp::GdipDeleteBrush(brush.cast());
                }
                gp::GdipDeletePath(path);
            }
            gp::GdipDeleteGraphics(graphics);
        }
    }
}
impl Drop for Artwork {
    fn drop(&mut self) {
        unsafe {
            let _ = RemoveFontMemResourceEx(self.brands);
            gp::GdiplusShutdown(self.token);
        }
    }
}
