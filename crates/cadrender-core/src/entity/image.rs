//! IMAGE: a raster picture (PNG, JPEG, ...) placed in the drawing, plus its
//! frame.
//!
//! The reader stores where the picture goes and the file name from its
//! IMAGEDEF. The picture itself is loaded after reading (cadrender-image,
//! called by `cadrender::Document::open`), because only then the folder of
//! the drawing is known. If the file is missing, only the frame is drawn,
//! like AutoCAD. Sinks that cannot draw pictures (`Sink::image` returns
//! false) also get only the frame.

use super::{Entity, EntityKind, PrepareCx};
use crate::geom::{Aff, BBox, PathData};
use crate::render::walk::{Visit, Walker};
use crate::sink::{ImageDraw, Sink};
use std::sync::Arc;

/// A decoded picture.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Picture {
    pub w: u32,
    pub h: u32,
    /// Straight (not premultiplied) RGBA, row by row from the top.
    pub rgba: Vec<u8>,
    /// Every pixel is grey (r = g = b).
    pub gray: bool,
    /// The file bytes as PNG or JPEG, for sinks that embed the file (SVG).
    pub encoded: Vec<u8>,
    /// MIME type of `encoded` ("image/png" or "image/jpeg").
    pub mime: &'static str,
}

#[derive(Clone, Debug)]
pub struct ImageEnt {
    /// Picture pixels (x right, y down, (0,0) = top-left corner, (w,h) =
    /// bottom-right corner) -> block coordinates.
    pub m: Aff,
    /// Size in pixels as the file says (IMAGE 13/23).
    pub size: [f64; 2],
    /// The frame and clip boundary, in block coordinates (closed).
    pub frame: PathData,
    /// Show the picture (IMAGE 70 bit 1).
    pub show: bool,
    /// 0..=100; AutoCAD's defaults are 50, 50, 0.
    pub brightness: u8,
    pub contrast: u8,
    pub fade: u8,
    /// File name as written in the IMAGEDEF.
    pub file: String,
    pub picture: Option<Arc<Picture>>,
}

impl ImageEnt {
    /// Matrix from the IMAGE groups: insertion point (lower-left corner),
    /// one pixel along x (`u`) and along y (`v`, upward), height in pixels.
    pub fn matrix(origin: [f64; 2], u: [f64; 2], v: [f64; 2], h: f64) -> Aff {
        Aff([u[0], u[1], -v[0], -v[1], origin[0] + v[0] * h, origin[1] + v[1] * h])
    }
}

impl EntityKind for ImageEnt {
    fn prepare(&mut self, _: &PrepareCx) -> BBox { self.frame.bbox() }

    fn draw<S: Sink>(&self, e: &Entity, v: &Visit, w: &mut Walker<'_, S>) {
        let (st, _) = w.style(e, v.layer, &v.inh, 0.0);
        if v.tiny {
            w.out.set_stroke(st);
            w.dot(&v.db);
            return;
        }
        if let (true, Some(pic)) = (self.show, &self.picture) {
            // the picture size in the file wins over the size in the IMAGE
            let sx = if pic.w > 0 { self.size[0] / pic.w as f64 } else { 1.0 };
            let sy = if pic.h > 0 { self.size[1] / pic.h as f64 } else { 1.0 };
            let m = v.m.mul(&self.m).mul(&Aff::scale(sx, sy));
            let mut clip: Vec<[f32; 2]> = Vec::with_capacity(self.frame.pts.len());
            for p in &self.frame.pts { let (x, y) = v.m.apply(p[0], p[1]); clip.push([x as f32, y as f32]); }
            let gray = w.opts.color == crate::color::ColorPolicy::MonochromeLight;
            let d = ImageDraw { pic, m, clip: &clip, brightness: self.brightness, contrast: self.contrast, fade: self.fade, gray, background: w.opts.background };
            w.out.flush();
            if w.out.sink.image(&d) { w.stats.drawn += 1; }
        }
        w.stroke(&self.frame, &v.m, st);
    }
}
