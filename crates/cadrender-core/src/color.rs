//! Colours: AutoCAD Color Index (ACI), entity colour values and colour policies.

/// An RGBA colour, not premultiplied.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Rgba { pub r: u8, pub g: u8, pub b: u8, pub a: u8 }

impl Rgba {
    pub const WHITE: Rgba = Rgba { r: 255, g: 255, b: 255, a: 255 };
    pub const BLACK: Rgba = Rgba { r: 0, g: 0, b: 0, a: 255 };
    pub const TRANSPARENT: Rgba = Rgba { r: 0, g: 0, b: 0, a: 0 };
    pub const fn rgb(rgb: u32) -> Rgba { Rgba { r: (rgb >> 16) as u8, g: (rgb >> 8) as u8, b: rgb as u8, a: 255 } }
    pub const fn gray(v: u8) -> Rgba { Rgba { r: v, g: v, b: v, a: 255 } }
    pub const fn with_alpha(self, a: u8) -> Rgba { Rgba { a, ..self } }
    pub fn is_gray(&self) -> bool { self.r == self.g && self.g == self.b }
}

/// The colour value of an entity, as stored in the file.
/// `0` = BYBLOCK, `256` = BYLAYER, `1..=255` = ACI, and `TRUE_COLOR | rgb` for a 24-bit colour.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct EntColor(pub i32);

impl EntColor {
    pub const BYBLOCK: EntColor = EntColor(0);
    pub const BYLAYER: EntColor = EntColor(256);
    pub const TRUE_COLOR: i32 = 0x100_0000;
    pub fn aci(i: i32) -> EntColor { EntColor(i.clamp(0, 256)) }
    pub fn true_color(rgb: u32) -> EntColor { EntColor(Self::TRUE_COLOR | (rgb & 0xff_ffff) as i32) }
    pub fn is_true_color(self) -> bool { self.0 & Self::TRUE_COLOR != 0 }
    /// Decode an AutoCAD "raw colour" (used in layer overrides and some objects):
    /// high byte 0xC2 = RGB, 0xC3 = ACI, 0xC0 = BYLAYER, 0xC1 = BYBLOCK.
    pub fn from_raw(raw: i32) -> Option<EntColor> {
        match (raw as u32) >> 24 {
            0xC0 => Some(Self::BYLAYER),
            0xC1 => Some(Self::BYBLOCK),
            0xC2 => Some(Self::true_color(raw as u32 & 0xff_ffff)),
            0xC3 => Some(Self::aci(raw & 0xff)),
            _ => None,
        }
    }
}

/// How file colours become picture colours.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ColorPolicy {
    /// Grey levels for a light background (ezdxf `MONOCHROME_LIGHT_BG`, the preview look).
    MonochromeLight,
    /// Real colours. ACI 7 is black on a light background and white on a dark one.
    TrueColor,
}

impl ColorPolicy {
    /// Map an RGB colour (no alpha) through the policy.
    pub fn map_rgb(self, rgb: u32, background: Rgba) -> Rgba {
        match self {
            ColorPolicy::MonochromeLight => Rgba::gray(gray_of(rgb)),
            ColorPolicy::TrueColor => {
                let c = Rgba::rgb(rgb);
                // very light colours vanish on a white page and very dark ones on black
                let lum = |c: Rgba| (c.r as u32 * 299 + c.g as u32 * 587 + c.b as u32 * 114) / 1000;
                if lum(background) > 127 && lum(c) > 245 { Rgba::BLACK } else if lum(background) <= 127 && lum(c) < 10 { Rgba::WHITE } else { c }
            }
        }
    }
    /// Map an ACI index (1..=255) through the policy.
    pub fn map_aci(self, aci: i32, background: Rgba) -> Rgba { self.map_rgb(aci_rgb(aci), background) }
}

/// ezdxf's `MONOCHROME_LIGHT_BG` grey: perceived brightness scaled to 70 %.
pub fn gray_of(rgb: u32) -> u8 {
    let (r, g, b) = (((rgb >> 16) & 255) as f64 / 255.0, ((rgb >> 8) & 255) as f64 / 255.0, (rgb & 255) as f64 / 255.0);
    let lum = ((0.299 * r * r + 0.587 * g * g + 0.114 * b * b).sqrt() * 1000.0).round() / 1000.0;
    (lum * 0.7 * 255.0).round().clamp(0.0, 255.0) as u8
}

/// RGB of an ACI index. ACI 7 ("white/black") is returned as black: the
/// colour policy decides the final look.
pub fn aci_rgb(c: i32) -> u32 { if c == 7 { 0 } else { ACI[c.clamp(0, 255) as usize] } }

/// AutoCAD Color Index table, index 0..=255, as 0xRRGGBB.
pub const ACI: [u32; 256] = [
    0x000000, 0xff0000, 0xffff00, 0x00ff00, 0x00ffff, 0x0000ff, 0xff00ff, 0xffffff,
    0x808080, 0xc0c0c0, 0xff0000, 0xff7f7f, 0xa50000, 0xa55252, 0x7f0000, 0x7f3f3f,
    0x4c0000, 0x4c2626, 0x260000, 0x261313, 0xff3f00, 0xff9f7f, 0xa52900, 0xa56752,
    0x7f1f00, 0x7f4f3f, 0x4c1300, 0x4c2f26, 0x260900, 0x261713, 0xff7f00, 0xffbf7f,
    0xa55200, 0xa57c52, 0x7f3f00, 0x7f5f3f, 0x4c2600, 0x4c3926, 0x261300, 0x261c13,
    0xffbf00, 0xffdf7f, 0xa57c00, 0xa59152, 0x7f5f00, 0x7f6f3f, 0x4c3900, 0x4c4226,
    0x261c00, 0x262113, 0xffff00, 0xffff7f, 0xa5a500, 0xa5a552, 0x7f7f00, 0x7f7f3f,
    0x4c4c00, 0x4c4c26, 0x262600, 0x262613, 0xbfff00, 0xdfff7f, 0x7ca500, 0x91a552,
    0x5f7f00, 0x6f7f3f, 0x394c00, 0x424c26, 0x1c2600, 0x212613, 0x7fff00, 0xbfff7f,
    0x52a500, 0x7ca552, 0x3f7f00, 0x5f7f3f, 0x264c00, 0x394c26, 0x132600, 0x1c2613,
    0x3fff00, 0x9fff7f, 0x29a500, 0x67a552, 0x1f7f00, 0x4f7f3f, 0x134c00, 0x2f4c26,
    0x092600, 0x172613, 0x00ff00, 0x7fff7f, 0x00a500, 0x52a552, 0x007f00, 0x3f7f3f,
    0x004c00, 0x264c26, 0x002600, 0x132613, 0x00ff3f, 0x7fff9f, 0x00a529, 0x52a567,
    0x007f1f, 0x3f7f4f, 0x004c13, 0x264c2f, 0x002609, 0x135817, 0x00ff7f, 0x7fffbf,
    0x00a552, 0x52a57c, 0x007f3f, 0x3f7f5f, 0x004c26, 0x264c39, 0x002613, 0x13581c,
    0x00ffbf, 0x7fffdf, 0x00a57c, 0x52a591, 0x007f5f, 0x3f7f6f, 0x004c39, 0x264c42,
    0x00261c, 0x135858, 0x00ffff, 0x7fffff, 0x00a5a5, 0x52a5a5, 0x007f7f, 0x3f7f7f,
    0x004c4c, 0x264c4c, 0x002626, 0x135858, 0x00bfff, 0x7fdfff, 0x007ca5, 0x5291a5,
    0x005f7f, 0x3f6f7f, 0x00394c, 0x26427e, 0x001c26, 0x135858, 0x007fff, 0x7fbfff,
    0x0052a5, 0x527ca5, 0x003f7f, 0x3f5f7f, 0x00264c, 0x26397e, 0x001326, 0x131c58,
    0x003fff, 0x7f9fff, 0x0029a5, 0x5267a5, 0x001f7f, 0x3f4f7f, 0x00134c, 0x262f7e,
    0x000926, 0x131758, 0x0000ff, 0x7f7fff, 0x0000a5, 0x5252a5, 0x00007f, 0x3f3f7f,
    0x00004c, 0x26267e, 0x000026, 0x131358, 0x3f00ff, 0x9f7fff, 0x2900a5, 0x6752a5,
    0x1f007f, 0x4f3f7f, 0x13004c, 0x2f267e, 0x090026, 0x171358, 0x7f00ff, 0xbf7fff,
    0x5200a5, 0x7c52a5, 0x3f007f, 0x5f3f7f, 0x26004c, 0x39267e, 0x130026, 0x1c1358,
    0xbf00ff, 0xdf7fff, 0x7c00a5, 0x9152a5, 0x5f007f, 0x6f3f7f, 0x39004c, 0x42264c,
    0x1c0026, 0x581358, 0xff00ff, 0xff7fff, 0xa500a5, 0xa552a5, 0x7f007f, 0x7f3f7f,
    0x4c004c, 0x4c264c, 0x260026, 0x581358, 0xff00bf, 0xff7fdf, 0xa5007c, 0xa55291,
    0x7f005f, 0x7f3f6f, 0x4c0039, 0x4c2642, 0x26001c, 0x581358, 0xff007f, 0xff7fbf,
    0xa50052, 0xa5527c, 0x7f003f, 0x7f3f5f, 0x4c0026, 0x4c2639, 0x260013, 0x58131c,
    0xff003f, 0xff7f9f, 0xa50029, 0xa55267, 0x7f001f, 0x7f3f4f, 0x4c0013, 0x4c262f,
    0x260009, 0x581317, 0x000000, 0x656565, 0x666666, 0x999999, 0xcccccc, 0xffffff,
];

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn monochrome_greys() {
        assert_eq!(gray_of(0), 0);
        assert_eq!(gray_of(0xffffff), 179); // 0.7 * 255
        assert_eq!(ColorPolicy::MonochromeLight.map_aci(7, Rgba::WHITE), Rgba::gray(0));
    }
    #[test]
    fn raw_colors() {
        assert_eq!(EntColor::from_raw(0xC3000005u32 as i32), Some(EntColor(5)));
        assert_eq!(EntColor::from_raw(0xC2112233u32 as i32), Some(EntColor::true_color(0x112233)));
        assert_eq!(EntColor::from_raw(0), None);
    }
    #[test]
    fn true_color_keeps_contrast() {
        assert_eq!(ColorPolicy::TrueColor.map_aci(7, Rgba::WHITE), Rgba::BLACK);
        assert_eq!(ColorPolicy::TrueColor.map_aci(7, Rgba::BLACK), Rgba::WHITE);
        assert_eq!(ColorPolicy::TrueColor.map_aci(1, Rgba::WHITE), Rgba::rgb(0xff0000));
    }
}
