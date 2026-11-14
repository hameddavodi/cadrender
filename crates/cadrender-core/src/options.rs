//! Render options: picture size, colours, lineweights, level of detail,
//! clean rules and limits. Everything a caller (CLI, Python) can change.
//!
//! Options never change the `Scene`. One scene can be rendered many times with
//! different options.

use crate::color::{ColorPolicy, Rgba};

/// All options for rendering one sheet.
#[derive(Clone, Debug)]
pub struct RenderOptions {
    /// Picture width in pixels. The height follows from the drawing extents.
    pub width: u32,
    /// "Dots per inch" used by the preview lineweight rule (mm handed to the
    /// rasteriser as points). 150 = what an ezdxf + matplotlib pipeline uses.
    pub dpi: f64,
    /// What the picture frame is: the drawing extents or the paper.
    pub framing: Framing,
    /// Margin around the drawing extents, as a part of the width/height (0.05 = 5 %).
    /// Not used with `Framing::Paper`.
    pub margin: f64,
    /// Highest picture height in pixels (very tall sheets are cut to this).
    pub max_height: u32,
    pub color: ColorPolicy,
    /// Page colour. Masks (WIPEOUT, MTEXT background) are drawn in this colour.
    pub background: Rgba,
    pub lineweight: Lineweights,
    pub lod: Lod,
    pub clean: CleanRules,
    pub limits: Limits,
    /// Respect the draw order table (SORTENTS). If false, file order is used.
    pub draw_order: bool,
    /// Draw MTEXT background masks.
    pub text_masks: bool,
    /// Draw WIPEOUT frames as the file says (WIPEOUTFRAME). `false` = never
    /// draw them, whatever the file says.
    pub wipeout_frames: bool,
    /// Draw viewports in parallel inside one sheet (rayon).
    pub parallel_viewports: bool,
}

impl Default for RenderOptions {
    /// Neutral defaults: true colour, paper lineweights, no clean rules.
    fn default() -> Self {
        RenderOptions {
            width: 2000,
            dpi: 150.0,
            framing: Framing::Extents,
            margin: 0.05,
            max_height: 20_000,
            color: ColorPolicy::TrueColor,
            background: Rgba::WHITE,
            lineweight: Lineweights::default(),
            lod: Lod::default(),
            clean: CleanRules::default(),
            limits: Limits::default(),
            draw_order: true,
            text_masks: true,
            wipeout_frames: true,
            parallel_viewports: true,
        }
    }
}

impl RenderOptions {
    /// Names accepted by [`RenderOptions::preset`].
    pub const PRESETS: [&'static str; 2] = ["none", "preview"];

    /// A named set of options. `"none"` = [`Default`]. `"preview"` = a print
    /// preview look: monochrome greys, true paper lineweights, and the
    /// `Defpoints` layer (never plotted by CAD programs) left out.
    pub fn preset(name: &str) -> Option<RenderOptions> {
        match name.to_ascii_lowercase().as_str() {
            "none" | "default" => Some(RenderOptions::default()),
            "preview" => Some(RenderOptions {
                color: ColorPolicy::MonochromeLight,
                clean: CleanRules::preview(),
                ..RenderOptions::default()
            }),
            _ => None,
        }
    }
}

/// The picture frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Framing {
    /// Extents of what is drawn (paper entities plus what each viewport shows),
    /// plus `margin`. Same as ezdxf + matplotlib.
    Extents,
    /// The paper outline of the layout (layout limits), like a plot. Falls back
    /// to `Extents` when the file has no valid limits.
    Paper,
}

/// How a lineweight (in 1/100 mm) becomes a pen width in pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum LineweightPolicy {
    /// ezdxf + matplotlib rule: the mm value is used as points at `dpi`.
    Preview,
    /// True plot width: mm on paper times pixels per paper mm.
    Paper,
    /// Every line is `min_px` wide.
    Hairline,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Lineweights {
    pub policy: LineweightPolicy,
    /// Lineweight for "default" (-3) and unset layers, in 1/100 mm.
    pub default_lw: i16,
    /// Thinnest pen in pixels.
    pub min_px: f64,
}

impl Default for Lineweights {
    fn default() -> Self { Lineweights { policy: LineweightPolicy::Paper, default_lw: 25, min_px: 1.0 } }
}

/// Level of detail: shortcuts for things too small to see. All sizes in pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Lod {
    /// Master switch. `false` = full detail.
    pub enabled: bool,
    /// Entities (and whole INSERTs) smaller than this become one dot.
    pub dot_px: f64,
    /// Text with a cap height below this is not drawn.
    pub text_hide_px: f64,
    /// Text with a cap height below this becomes a grey bar ("greeking").
    pub text_greek_px: f64,
    /// Hatch pattern lines closer than this become a grey tint.
    pub hatch_tint_px: f64,
    /// Dash patterns with a period below this are drawn solid. Always used,
    /// also in full detail (a 1 px dash pattern is only noise).
    pub dash_min_period_px: f64,
}

impl Default for Lod {
    fn default() -> Self { Lod { enabled: true, dot_px: 0.75, text_hide_px: 0.6, text_greek_px: 2.2, hatch_tint_px: 1.5, dash_min_period_px: 3.0 } }
}

impl Lod {
    /// Full detail: no dots, no greeking, no tint.
    pub fn full() -> Lod { Lod { enabled: false, ..Lod::default() } }
}

/// What to leave out of the picture ("clean"). Matching ignores case and
/// separators: `Title_Block-Text`, `title block text` and `TITLEBLOCKTEXT`
/// are the same name.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CleanRules {
    /// Layers to drop. Entities on these layers are not drawn, ATTRIBs on
    /// them too. VIEWPORTs are never dropped.
    pub layers: Vec<String>,
    /// Drop INSERTs whose block name contains one of these keywords.
    pub block_keywords: Vec<String>,
}

impl CleanRules {
    /// Rules of the `preview` preset: drop the `Defpoints` layer.
    pub fn preview() -> CleanRules {
        CleanRules { layers: vec!["Defpoints".to_string()], block_keywords: Vec::new() }
    }
    pub fn is_empty(&self) -> bool { self.layers.is_empty() && self.block_keywords.is_empty() }
    pub fn drops_layer(&self, name: &str) -> bool {
        let n = normalize(name);
        self.layers.iter().any(|l| normalize(l) == n)
    }
    pub fn drops_block(&self, name: &str) -> bool {
        let n = normalize(name);
        self.block_keywords.iter().any(|k| { let k = normalize(k); !k.is_empty() && n.contains(&k) })
    }
}

/// Lower case, letters and digits only.
pub fn normalize(name: &str) -> String { name.chars().filter(|c| c.is_alphanumeric()).flat_map(|c| c.to_lowercase()).collect() }

/// Safety limits against broken or hostile files.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Limits {
    /// Deepest INSERT nesting that is followed.
    pub max_depth: u32,
    /// Most entity visits for one sheet. The sheet stops there and gets a warning.
    pub max_visits: u64,
}

impl Default for Limits {
    fn default() -> Self { Limits { max_depth: 24, max_visits: 200_000_000 } }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layers_match_any_spelling() {
        let r = CleanRules { layers: vec!["Title_Block".into(), "Defpoints".into()], block_keywords: vec![] };
        for n in ["Title_Block", "title-BLOCK", "title block", "TITLEBLOCK", "defpoints"] {
            assert!(r.drops_layer(n), "{n}");
        }
        assert!(!r.drops_layer("Title_Block2"));
        assert!(!r.drops_layer("0"));
        assert!(CleanRules::preview().drops_layer("DEFPOINTS"));
    }

    #[test]
    fn blocks_by_keyword() {
        let r = CleanRules { layers: vec![], block_keywords: vec!["title block".into(), "company logo".into()] };
        assert!(r.drops_block("A3_TITLE_BLOCK_v2"));
        assert!(r.drops_block("Company_Logo"));
        assert!(r.drops_block("company-logo ltd"));
        assert!(!r.drops_block("window"));
    }

    #[test]
    fn presets() {
        let t = RenderOptions::preset("PREVIEW").unwrap();
        assert_eq!(t.color, ColorPolicy::MonochromeLight);
        assert!(!t.clean.is_empty());
        assert!(RenderOptions::preset("none").unwrap().clean.is_empty());
        assert!(RenderOptions::preset("nope").is_none());
    }
}
