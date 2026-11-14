//! Rendering one sheet: frame it, draw the paper-space entities, then draw
//! model space through every viewport (in parallel), each into its own region.

pub mod batch;
pub mod index;
pub mod prep;
pub mod walk;

use crate::color::Rgba;
use crate::geom::{Aff, BBox};
use crate::options::RenderOptions;
use crate::scene::{Layout, Scene};
use crate::sink::{IRect, Sink};
use batch::Batcher;
pub use prep::Prepared;
use walk::{Inherit, Walker, Warnings};

/// Counters for one sheet.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Stats {
    /// Entities looked at.
    pub visited: u64,
    /// Entities drawn.
    pub drawn: u64,
    /// Entities skipped because their box is outside the view.
    pub culled: u64,
    /// Entities drawn as a dot or a bar, or hidden as too small.
    pub tiny: u64,
    pub hatch_lines: u64,
    /// Hatch line families drawn as a tint.
    pub hatch_dense: u64,
    pub glyphs: u64,
    /// Draw calls on the sink.
    pub flushes: u64,
}

impl Stats {
    pub fn add(&mut self, o: &Stats) {
        self.visited += o.visited; self.drawn += o.drawn; self.culled += o.culled; self.tiny += o.tiny;
        self.hatch_lines += o.hatch_lines; self.hatch_dense += o.hatch_dense; self.glyphs += o.glyphs; self.flushes += o.flushes;
    }
}

/// A rendered sheet.
pub struct SheetRender<S> {
    pub sink: S,
    pub width: u32,
    pub height: u32,
    /// Nothing visible on the sheet: the page is blank.
    pub empty: bool,
    pub viewports: usize,
    pub stats: Stats,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum RenderError {
    NoSuchSheet(String),
    InvalidOptions(String),
}

impl std::fmt::Display for RenderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RenderError::NoSuchSheet(s) => write!(f, "no sheet named {s:?}"),
            RenderError::InvalidOptions(s) => write!(f, "invalid options: {s}"),
        }
    }
}
impl std::error::Error for RenderError {}

/// Render the sheet called `name`.
pub fn render_sheet<S: Sink>(scene: &Scene, name: &str, opts: &RenderOptions, prep: &Prepared, cfg: &S::Config) -> Result<SheetRender<S>, RenderError> {
    let layout = scene.layout(name).ok_or_else(|| RenderError::NoSuchSheet(name.to_string()))?;
    render_layout(scene, layout, opts, prep, cfg)
}

/// Sheet extents in paper units: visible paper-space entities plus what each
/// viewport really shows.
fn extents(scene: &Scene, layout: &Layout, opts: &RenderOptions, prep: &Prepared) -> BBox {
    let block = &scene.blocks[layout.block as usize];
    let model = scene.model_block();
    let paper_view = prep.view(scene, opts, None);
    let mut ext = BBox::EMPTY;
    for e in &block.ents {
        if matches!(e.kind, crate::entity::Kind::Viewport(_)) || paper_view.hidden[e.props.layer as usize] { continue; }
        if let Some(b) = prep.frame_bbox(e) { ext.union(&b); }
    }
    for v in scene.viewports(layout) {
        let view = prep.view(scene, opts, Some(v));
        let m = v.matrix();
        let rect = v.visible_rect();
        // without twist the viewport is a box in model space too: reject there, cheaply
        let model_rect = if v.twist.rem_euclid(360.0) < 1e-12 && v.is_plan() { Some(rect.transformed(&m.inverse())) } else { None };
        let mut vext = BBox::EMPTY;
        // only the entities near the window (grid), else all of them
        let window = rect.transformed(&m.inverse());
        let near = prep.model_index.as_ref().filter(|_| window.x0.is_finite() && window.y0.is_finite() && window.x1.is_finite() && window.y1.is_finite())
            .map(|ix| { let mut v = vec![]; ix.query(&window, &mut v); v });
        let all = 0..if near.is_some() { 0 } else { model.ents.len() as u32 };
        for i in near.iter().flatten().copied().chain(all) {
            let (i, e) = (i as usize, &model.ents[i as usize]);
            if view.hidden[e.props.layer as usize] { continue; }
            let Some(b) = prep.model_frame_bbox(i, e) else { continue };
            let b = match &model_rect {
                Some(r) => { let c = b.intersect(r); if c.is_empty() { continue; } c.transformed(&m).intersect(&rect) }
                None => b.transformed(&m).intersect(&rect),
            };
            if !b.is_empty() {
                vext.union(&b);
                if vext.contains(&rect) { break; } // cannot grow any more
            }
        }
        ext.union(&vext);
    }
    ext
}

/// One drawn viewport: region sink, its place, counters, clip polygon.
type VpOut<S> = (S, IRect, Stats, Warnings, Option<Vec<[f32; 2]>>);

/// Clip polygon of a non-rectangular viewport, in pixels of its region
/// (whose top-left corner is page pixel `ix0`, `iy0`).
fn clip_poly(v: &crate::entity::viewport::ViewportEnt, mp: &Aff, ix0: i32, iy0: i32) -> Option<Vec<[f32; 2]>> {
    let c = v.clip.as_ref()?;
    Some(c.iter().map(|p| { let (x, y) = mp.apply(p[0], p[1]); [(x - ix0 as f64) as f32, (y - iy0 as f64) as f32] }).collect())
}

/// Render one layout.
pub fn render_layout<S: Sink>(scene: &Scene, layout: &Layout, opts: &RenderOptions, prep: &Prepared, cfg: &S::Config) -> Result<SheetRender<S>, RenderError> {
    let mut page = None;
    let r = render_layout_strips(scene, layout, opts, prep, cfg, u32::MAX, |s: S, _, _| page = Some(s))?;
    let sink = page.expect("a sheet has at least one strip");
    Ok(SheetRender { sink, width: r.width, height: r.height, empty: r.empty, viewports: r.viewports, stats: r.stats, warnings: r.warnings })
}

/// Render the sheet called `name` in strips of at most `rows` pixel rows,
/// top to bottom. Each strip is a page of its own (full width) handed to
/// `f(strip, first_row, picture_height)`. The pixels are the same as one `render_sheet` call,
/// but only one strip is in memory at a time: this is how very big pictures
/// (10 000 px and more) keep their memory bounded.
pub fn render_sheet_strips<S: Sink>(scene: &Scene, name: &str, opts: &RenderOptions, prep: &Prepared, cfg: &S::Config, rows: u32, f: impl FnMut(S, u32, u32)) -> Result<SheetRender<()>, RenderError> {
    let layout = scene.layout(name).ok_or_else(|| RenderError::NoSuchSheet(name.to_string()))?;
    render_layout_strips(scene, layout, opts, prep, cfg, rows, f)
}

/// Size and paper -> pixel matrix of a sheet.
struct Frame { w: u32, h: u32, mp: Aff, scale: f64, empty: bool }

fn frame(scene: &Scene, layout: &Layout, opts: &RenderOptions, prep: &Prepared) -> Result<Frame, RenderError> {
    if opts.width == 0 || opts.width > 100_000 { return Err(RenderError::InvalidOptions(format!("width {} must be 1..=100000", opts.width))); }
    if !(opts.dpi > 0.0 && opts.dpi.is_finite()) { return Err(RenderError::InvalidOptions("dpi must be > 0".into())); }
    let ext = match (opts.framing, layout.paper.limits) {
        (crate::options::Framing::Paper, Some(l)) => l,
        _ => extents(scene, layout, opts, prep),
    };
    let margin = if opts.framing == crate::options::Framing::Paper && layout.paper.limits.is_some() { 0.0 } else { opts.margin };
    if ext.is_empty() || ext.w() <= 0.0 || !ext.w().is_finite() || !ext.h().is_finite() {
        // blank A4-shaped page, so callers always get a picture
        let (w, h) = (opts.width, ((opts.width as f64) * 210.0 / 297.0).round().max(1.0) as u32);
        return Ok(Frame { w, h, mp: Aff::ID, scale: 1.0, empty: true });
    }
    let (mx, my) = (ext.w() * margin, ext.h() * margin);
    let scale = opts.width as f64 / (ext.w() + 2.0 * mx);
    let (w, h) = (opts.width, (((ext.h() + 2.0 * my) * scale).ceil() as u32).clamp(1, opts.max_height.max(1)));
    // paper -> pixels (y down)
    let mp = Aff([scale, 0.0, 0.0, -scale, -(ext.x0 - mx) * scale, (ext.y1 + my) * scale]);
    Ok(Frame { w, h, mp, scale, empty: false })
}

fn render_layout_strips<S: Sink>(scene: &Scene, layout: &Layout, opts: &RenderOptions, prep: &Prepared, cfg: &S::Config, rows: u32, mut f: impl FnMut(S, u32, u32)) -> Result<SheetRender<()>, RenderError> {
    let fr = frame(scene, layout, opts, prep)?;
    let (w, full_h, scale) = (fr.w, fr.h, fr.scale);
    let rows = rows.clamp(1, full_h);
    if fr.empty {
        let mut y0 = 0;
        while y0 < full_h { let sh = rows.min(full_h - y0); f(S::page(cfg, w, sh, opts.background), y0, full_h); y0 += sh; }
        return Ok(SheetRender { sink: (), width: w, height: full_h, empty: true, viewports: 0, stats: Stats::default(), warnings: vec![] });
    }
    let block = &scene.blocks[layout.block as usize];
    let psltscale = layout.paper.psltscale.unwrap_or(scene.header.psltscale);
    let upm = layout.paper.units_per_mm;
    let top = Inherit { color: if opts.background == Rgba::BLACK { Rgba::WHITE } else { Rgba::BLACK }, ..Inherit::top() };
    let paper_view = prep.view(scene, opts, None);
    let vps: Vec<_> = scene.viewports(layout).collect();
    let mut stats = Stats::default();
    let mut warnings = Warnings::default();
    // With more than one strip, things just outside a strip can still reach
    // into it (thick lines): cull with a wider window. 2.11 mm is the widest
    // lineweight; 1 % of the width covers wide polylines.
    let seam = if rows < full_h { (2.11 * upm * scale).max(2.11 * opts.dpi / 72.0).max(0.01 * w as f64) + 4.0 } else { 0.0 };
    let mut y0 = 0;
    while y0 < full_h {
        let h = rows.min(full_h - y0);
        let mp = Aff::translate(0.0, -(y0 as f64)).mul(&fr.mp);
        let mut wk = Walker {
            scene, opts, prep, view: &paper_view,
            out: Batcher::new(S::page(cfg, w, h, opts.background)),
            clip: BBox::new(0.0, -seam, w as f64, h as f64 + seam),
            paper_scale: scale, dash_scale: scale, paper_units_per_mm: upm,
            stats: Stats::default(), warnings: Warnings::default(),
        };
        wk.walk(&block.ents, block.file_order.as_deref(), &mp, &top, 0);
        stats.add(&wk.stats);
        warnings.merge(wk.warnings);
        stats.flushes += wk.out.flushes;
        let mut page = wk.out.finish();

        let draw_vp = |page: &S, v: &crate::entity::viewport::ViewportEnt| -> Option<VpOut<S>> {
            let m = mp.mul(&v.matrix());
            let r = v.visible_rect().transformed(&mp);
            let (ix0, iy0) = (r.x0.floor().max(0.0) as i32, r.y0.floor().max(0.0) as i32);
            let (ix1, iy1) = ((r.x1.ceil() as i32).min(w as i32), (r.y1.ceil() as i32).min(h as i32));
            if ix1 <= ix0 || iy1 <= iy0 { return None; }
            let rect = IRect { x: ix0, y: iy0, w: (ix1 - ix0) as u32, h: (iy1 - iy0) as u32 };
            let mt = Aff::translate(-ix0 as f64, -iy0 as f64).mul(&m);
            let mut clip = BBox::new(r.x0 - ix0 as f64, r.y0 - iy0 as f64, r.x1 - ix0 as f64, r.y1 - iy0 as f64);
            if seam > 0.0 { clip = clip.intersect(&BBox::new(-seam, -seam, rect.w as f64 + seam, rect.h as f64 + seam)); }
            let view = prep.view(scene, opts, Some(v));
            let dash_scale = if psltscale { scale } else { scale * v.scale() };
            let mut wk = Walker {
                scene, opts, prep, view: &view,
                out: Batcher::new(page.region(rect)),
                clip, paper_scale: scale, dash_scale, paper_units_per_mm: upm,
                stats: Stats::default(), warnings: Warnings::default(),
            };
            let model = scene.model_block();
            wk.walk(&model.ents, model.file_order.as_deref(), &mt, &top, 0);
            let mut st = wk.stats;
            st.flushes += wk.out.flushes;
            let wn = wk.warnings;
            Some((wk.out.finish(), rect, st, wn, clip_poly(v, &mp, ix0, iy0)))
        };
        // Regions go on the page in viewport order. At most one region per worker
        // thread is alive at a time, so big pictures keep their memory bounded.
        let batch = if opts.parallel_viewports { rayon::current_num_threads().max(1) } else { 1 };
        for chunk in vps.chunks(batch) {
            let subs: Vec<Option<VpOut<S>>> = if chunk.len() > 1 {
                use rayon::prelude::*;
                chunk.par_iter().map(|v| draw_vp(&page, v)).collect()
            } else {
                chunk.iter().map(|v| draw_vp(&page, v)).collect()
            };
            for (sub, rect, st, wn, clip) in subs.into_iter().flatten() {
                stats.add(&st);
                warnings.merge(wn);
                page.composite(sub, rect, clip.as_deref());
            }
        }
        f(page, y0, full_h);
        y0 += h;
    }
    Ok(SheetRender { sink: (), width: w, height: full_h, empty: false, viewports: vps.len(), stats, warnings: warnings.messages() })
}
