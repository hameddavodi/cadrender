//! Python options (a plain dict) -> `RenderOptions`.
//!
//! The Python side (`RenderOptions.to_dict()` plus keyword arguments) hands
//! over a dict. Every key is checked here, so a typo is an `OptionsError`,
//! never silently ignored.

use crate::errors;
use cadrender::RenderOptions;
use cadrender_core::{CleanRules, ColorPolicy, Framing, LineweightPolicy, Lod, Rgba};
use pyo3::prelude::*;
use pyo3::types::PyDict;

/// All keys we accept. Kept in one list for the error message.
pub const KEYS: &[&str] = &[
    "preset", "width", "dpi", "color", "background", "lineweight", "default_lw", "min_lw_px", "frame", "margin",
    "max_height", "full_detail", "lod", "clean", "draw_order", "text_masks", "wipeout_frames", "parallel_viewports", "max_depth", "max_visits",
];

const LOD_KEYS: &[&str] = &["enabled", "dot_px", "text_hide_px", "text_greek_px", "hatch_tint_px", "dash_min_period_px"];

/// Build options from a dict. `preset` is applied first, then the other keys.
pub fn from_dict(py: Python<'_>, d: Option<&Bound<'_, PyDict>>) -> PyResult<RenderOptions> {
    let get = |k: &str| -> PyResult<Option<Bound<'_, PyAny>>> {
        match d { Some(d) => Ok(d.get_item(k)?.filter(|v| !v.is_none())), None => Ok(None) }
    };
    if let Some(d) = d {
        for k in d.keys() {
            let k: String = k.extract()?;
            if !KEYS.contains(&k.as_str()) {
                return Err(errors::options(py, format!("unknown option {k:?}; known options: {}", KEYS.join(", "))));
            }
        }
    }
    let preset: String = match get("preset")? { Some(v) => v.extract()?, None => "preview".into() };
    let mut o = RenderOptions::preset(&preset)
        .ok_or_else(|| errors::options(py, format!("unknown preset {preset:?}; use one of {:?}", RenderOptions::PRESETS)))?;

    if let Some(v) = get("width")? { o.width = positive(py, "width", v.extract()?)?; }
    if let Some(v) = get("dpi")? { o.dpi = v.extract()?; }
    if let Some(v) = get("color")? {
        o.color = match v.extract::<String>()?.to_ascii_lowercase().as_str() {
            "mono" | "monochrome" | "grey" | "gray" => ColorPolicy::MonochromeLight,
            "true" | "truecolor" | "color" | "colour" => ColorPolicy::TrueColor,
            x => return Err(errors::options(py, format!("unknown color {x:?}; use \"mono\" or \"true\""))),
        };
    }
    if let Some(v) = get("background")? { o.background = color(py, &v)?; }
    if let Some(v) = get("lineweight")? {
        o.lineweight.policy = match v.extract::<String>()?.to_ascii_lowercase().as_str() {
            "paper" => LineweightPolicy::Paper,
            "preview" => LineweightPolicy::Preview,
            "hairline" => LineweightPolicy::Hairline,
            x => return Err(errors::options(py, format!("unknown lineweight {x:?}; use \"paper\", \"preview\" or \"hairline\""))),
        };
    }
    if let Some(v) = get("default_lw")? { o.lineweight.default_lw = v.extract()?; }
    if let Some(v) = get("min_lw_px")? { o.lineweight.min_px = v.extract()?; }
    if let Some(v) = get("frame")? {
        o.framing = match v.extract::<String>()?.to_ascii_lowercase().as_str() {
            "extents" => Framing::Extents,
            "paper" => Framing::Paper,
            x => return Err(errors::options(py, format!("unknown frame {x:?}; use \"extents\" or \"paper\""))),
        };
    }
    if let Some(v) = get("margin")? { o.margin = v.extract()?; }
    if let Some(v) = get("max_height")? { o.max_height = positive(py, "max_height", v.extract()?)?; }
    if let Some(v) = get("full_detail")? { if v.extract::<bool>()? { o.lod = Lod::full(); } }
    if let Some(v) = get("lod")? { lod(py, &v, &mut o.lod)?; }
    if let Some(v) = get("clean")? { o.clean = clean(py, &v, &o.clean)?; }
    if let Some(v) = get("draw_order")? { o.draw_order = v.extract()?; }
    if let Some(v) = get("text_masks")? { o.text_masks = v.extract()?; }
    if let Some(v) = get("wipeout_frames")? { o.wipeout_frames = v.extract()?; }
    if let Some(v) = get("parallel_viewports")? { o.parallel_viewports = v.extract()?; }
    if let Some(v) = get("max_depth")? { o.limits.max_depth = v.extract()?; }
    if let Some(v) = get("max_visits")? { o.limits.max_visits = v.extract()?; }
    Ok(o)
}

fn positive(py: Python<'_>, name: &str, v: i64) -> PyResult<u32> {
    if v <= 0 || v > 100_000 { return Err(errors::options(py, format!("{name} must be between 1 and 100000, got {v}"))); }
    Ok(v as u32)
}

/// `"#RRGGBB"`, `"#RRGGBBAA"`, `"white"`, `"black"`, `"transparent"` or a
/// tuple `(r, g, b)` / `(r, g, b, a)` with values 0..255.
fn color(py: Python<'_>, v: &Bound<'_, PyAny>) -> PyResult<Rgba> {
    let bad = || errors::options(py, format!("bad background {v}; use \"#RRGGBB\", \"#RRGGBBAA\", \"white\", \"black\", \"transparent\" or (r, g, b[, a])"));
    if let Ok(s) = v.extract::<String>() {
        return match s.to_ascii_lowercase().as_str() {
            "white" => Ok(Rgba::WHITE),
            "black" => Ok(Rgba::BLACK),
            "transparent" | "none" => Ok(Rgba::TRANSPARENT),
            h if h.starts_with('#') && (h.len() == 7 || h.len() == 9) => {
                let b = |i: usize| u8::from_str_radix(&h[i..i + 2], 16).map_err(|_| bad());
                Ok(Rgba { r: b(1)?, g: b(3)?, b: b(5)?, a: if h.len() == 9 { b(7)? } else { 255 } })
            }
            _ => Err(bad()),
        };
    }
    let t: Vec<u8> = v.extract().map_err(|_| bad())?;
    match t.as_slice() {
        [r, g, b] => Ok(Rgba { r: *r, g: *g, b: *b, a: 255 }),
        [r, g, b, a] => Ok(Rgba { r: *r, g: *g, b: *b, a: *a }),
        _ => Err(bad()),
    }
}

/// `lod={"dot_px": 1.0, ...}`: change single level-of-detail limits.
fn lod(py: Python<'_>, v: &Bound<'_, PyAny>, l: &mut Lod) -> PyResult<()> {
    let d = v.cast::<PyDict>().map_err(|_| errors::options(py, "lod must be a dict"))?;
    for (k, v) in d.iter() {
        let k: String = k.extract()?;
        match k.as_str() {
            "enabled" => l.enabled = v.extract()?,
            "dot_px" => l.dot_px = v.extract()?,
            "text_hide_px" => l.text_hide_px = v.extract()?,
            "text_greek_px" => l.text_greek_px = v.extract()?,
            "hatch_tint_px" => l.hatch_tint_px = v.extract()?,
            "dash_min_period_px" => l.dash_min_period_px = v.extract()?,
            _ => return Err(errors::options(py, format!("unknown lod key {k:?}; known: {}", LOD_KEYS.join(", ")))),
        }
    }
    Ok(())
}

/// `clean=False` (no rules), `True` (the preset's rules) or
/// `{"layers": [...], "block_keywords": [...]}` (your own rules).
fn clean(py: Python<'_>, v: &Bound<'_, PyAny>, preset: &CleanRules) -> PyResult<CleanRules> {
    if let Ok(b) = v.extract::<bool>() { return Ok(if b { preset.clone() } else { CleanRules::default() }); }
    let d = v.cast::<PyDict>().map_err(|_| errors::options(py, "clean must be True, False or a dict with \"layers\" and \"block_keywords\""))?;
    let mut r = CleanRules::default();
    for (k, v) in d.iter() {
        let k: String = k.extract()?;
        let list: Vec<String> = v.extract().map_err(|_| errors::options(py, format!("clean[{k:?}] must be a list of strings")))?;
        match k.as_str() {
            "layers" => r.layers = list,
            "block_keywords" => r.block_keywords = list,
            _ => return Err(errors::options(py, format!("unknown clean key {k:?}; use \"layers\" and \"block_keywords\""))),
        }
    }
    Ok(r)
}
