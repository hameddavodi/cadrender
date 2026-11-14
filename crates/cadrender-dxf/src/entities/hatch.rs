//! HATCH: boundary loops, hatch style, pattern definition and gradient.
//!
//! The groups are read in file order with exact counts (a spline edge has
//! its own group 97, so a "read until the next 97" parser goes wrong there).
//! The geometry work (edge chaining, islands, style) is in
//! `cadrender_core::entity::hatch::boundary`, shared with the DWG reader.

use super::Common;
use crate::tags::{f, i, long, s, Tag};
use cadrender_core::color::EntColor;
use cadrender_core::entity::hatch::boundary::{join_loops, loop_from_edges, loop_from_polyline};
use cadrender_core::entity::hatch::{BoundaryLoop, Edge, Gradient, GradientKind, HatchEnt, HatchStyle, PatLine};
use cadrender_core::entity::Kind;

pub fn hatch(body: &[Tag], com: &Common) -> Option<Kind> {
    let mut h = parse(body);
    if h.path.is_empty() { return None; }
    if let Some(o) = com.ocs() {
        h.path.transform(&o);
        for p in &mut h.pattern { *p = p.transformed(&o); }
        if let Some(g) = &mut h.gradient { g.angle = std::f64::consts::PI - g.angle; }
    }
    Some(h.into())
}

/// A cursor over the groups of one record.
struct Cur<'a, 'b> { t: &'a [Tag<'b>], p: usize }

impl<'a, 'b> Cur<'a, 'b> {
    fn peek(&self) -> Option<i32> { self.t.get(self.p).map(|t| t.0) }
    fn next(&mut self) -> Option<Tag<'b>> { let t = self.t.get(self.p).copied(); self.p += 1; t }
    /// The value of the next group if it has `code` (then it is consumed).
    fn take(&mut self, code: i32) -> Option<&'b [u8]> {
        match self.t.get(self.p) { Some(&(c, v)) if c == code => { self.p += 1; Some(v) } _ => None }
    }
    fn takef(&mut self, code: i32) -> f64 { self.take(code).map(f).unwrap_or(0.0) }
    /// Skip to the group `code` and consume it; stop (without consuming) at a group in `stop`.
    fn seek(&mut self, code: i32, stop: &[i32]) -> Option<&'b [u8]> {
        while let Some(c) = self.peek() {
            if c == code { return self.next().map(|t| t.1); }
            if stop.contains(&c) { return None; }
            self.p += 1;
        }
        None
    }
    fn point(&mut self, cx: i32) -> [f64; 2] { let x = self.takef(cx); let y = self.takef(cx + 10); [x, y] }
}

/// Groups that end the loops part of a HATCH.
const AFTER_LOOPS: [i32; 6] = [75, 76, 78, 98, 450, 470];

fn parse(body: &[Tag]) -> HatchEnt {
    let mut c = Cur { t: body, p: 0 };
    let mut solid = false;
    let mut nloops = 0;
    while let Some((code, v)) = c.next() {
        match code { 70 => solid = i(v) != 0, 91 => { nloops = i(v).clamp(0, 1_000_000); break; } _ => {} }
    }
    let mut loops = Vec::with_capacity(nloops as usize);
    for _ in 0..nloops {
        let Some(fl) = c.seek(92, &AFTER_LOOPS) else { break };
        let flags = i(fl) as u32;
        let path = if flags & 2 != 0 { polyline_loop(&mut c) } else { loop_from_edges(&edges(&mut c)) };
        // source boundary objects
        if let Some(n) = c.take(97) { for _ in 0..i(n).clamp(0, 1_000_000) { if c.take(330).is_none() { break; } } }
        loops.push(BoundaryLoop { flags, path });
    }
    let mut style = HatchStyle::Normal;
    let mut double = false;
    let mut pattern: Vec<PatLine> = vec![];
    let mut in_pattern = false;
    let mut grad = false;
    let mut g = Gradient { kind: GradientKind::Linear, colors: [EntColor::aci(5), EntColor::aci(2)], two_colors: true, angle: 0.0, shift: 0.0, tint: 0.5 };
    let mut ncol = 0usize;
    let mut one_color = false;
    while let Some((code, v)) = c.next() {
        match code {
            75 => style = HatchStyle::from_code(i(v)),
            77 => double = i(v) != 0,
            78 => in_pattern = true,
            98 => in_pattern = false,
            53 if in_pattern => pattern.push(PatLine { angle: f(v).to_radians(), ..PatLine::default() }),
            43 if in_pattern => if let Some(p) = pattern.last_mut() { p.base[0] = f(v) },
            44 if in_pattern => if let Some(p) = pattern.last_mut() { p.base[1] = f(v) },
            45 if in_pattern => if let Some(p) = pattern.last_mut() { p.offset[0] = f(v) },
            46 if in_pattern => if let Some(p) = pattern.last_mut() { p.offset[1] = f(v) },
            49 if in_pattern => if let Some(p) = pattern.last_mut() { p.dashes.push(f(v)) },
            450 => { grad = i(v) != 0; in_pattern = false; }
            452 => one_color = i(v) != 0,
            460 => g.angle = f(v),
            461 => g.shift = f(v),
            462 => g.tint = f(v),
            463 => ncol += 1,
            63 if grad && (1..=2).contains(&ncol) => g.colors[ncol - 1] = EntColor::aci(i(v).abs()),
            421 if grad && (1..=2).contains(&ncol) => g.colors[ncol - 1] = EntColor::true_color(long(v) as u32 & 0xff_ffff),
            470 => if let Some(k) = GradientKind::from_name(&s(v)) { g.kind = k },
            1001 => break, // XDATA
            _ => {}
        }
    }
    // a double user hatch stored with one line family: add the crossing family
    if double && pattern.len() == 1 {
        let p = pattern[0].clone();
        pattern.push(PatLine { angle: p.angle + std::f64::consts::FRAC_PI_2, offset: [-p.offset[1], p.offset[0]], ..p });
    }
    g.two_colors = !(one_color && ncol < 2);
    let gradient = if grad { Some(Box::new(g)) } else { None };
    HatchEnt { path: join_loops(loops, style), solid, pattern, gradient }
}

fn polyline_loop(c: &mut Cur) -> cadrender_core::geom::PathData {
    c.take(72); // has bulges: a 42 after a vertex is read either way
    c.take(73);
    let n = c.take(93).map(i).unwrap_or(0).clamp(0, 10_000_000) as usize;
    let mut pts: Vec<(f64, f64, f64)> = Vec::with_capacity(n);
    while pts.len() < n && c.peek() == Some(10) {
        let [x, y] = c.point(10);
        pts.push((x, y, c.takef(42)));
    }
    loop_from_polyline(&pts)
}

fn edges(c: &mut Cur) -> Vec<Edge> {
    let n = c.take(93).map(i).unwrap_or(0).clamp(0, 10_000_000) as usize;
    let mut out = Vec::with_capacity(n);
    for _ in 0..n {
        let Some(t) = c.take(72) else { break };
        match i(t) {
            1 => { let a = c.point(10); let b = c.point(11); out.push(Edge::Line { a, b }); }
            2 => {
                let ctr = c.point(10);
                let r = c.takef(40);
                let (a0, a1) = (c.takef(50).to_radians(), c.takef(51).to_radians());
                let ccw = c.take(73).map(i).unwrap_or(1) != 0;
                out.push(Edge::Arc { c: ctr, r, a0, a1, ccw });
            }
            3 => {
                let ctr = c.point(10);
                let major = c.point(11);
                let ratio = c.takef(40);
                let (a0, a1) = (c.takef(50).to_radians(), c.takef(51).to_radians());
                let ccw = c.take(73).map(i).unwrap_or(1) != 0;
                out.push(Edge::Ellipse { c: ctr, major, ratio, a0, a1, ccw });
            }
            4 => out.push(spline_edge(c)),
            _ => {}
        }
        // tolerate unknown extra groups inside an edge
        while let Some(code) = c.peek() { if matches!(code, 72 | 92 | 97) || AFTER_LOOPS.contains(&code) { break; } c.p += 1; }
    }
    out
}

fn spline_edge(c: &mut Cur) -> Edge {
    let degree = c.take(94).map(i).unwrap_or(3).clamp(1, 32) as usize;
    let rational = c.take(73).map(i).unwrap_or(0) != 0;
    c.take(74);
    let nk = c.take(95).map(i).unwrap_or(0).clamp(0, 1_000_000) as usize;
    let nc = c.take(96).map(i).unwrap_or(0).clamp(0, 1_000_000) as usize;
    let mut knots = Vec::with_capacity(nk);
    while knots.len() < nk { match c.take(40) { Some(v) => knots.push(f(v)), None => break } }
    let (mut ctrl, mut weights) = (Vec::with_capacity(nc), vec![]);
    while ctrl.len() < nc && c.peek() == Some(10) {
        ctrl.push(c.point(10));
        if let Some(w) = c.take(42) { weights.push(f(w)); } else if rational { weights.push(1.0); }
    }
    let mut fit = vec![];
    // fit points (R2010+): 97 count, 11/21 points, then 12/22 and 13/23 tangents
    if c.peek() == Some(97) && c.t.get(c.p + 1).map(|t| t.0) == Some(11) {
        let nf = c.take(97).map(i).unwrap_or(0).clamp(0, 1_000_000) as usize;
        while fit.len() < nf && c.peek() == Some(11) { fit.push(c.point(11)); }
        if c.peek() == Some(12) { c.point(12); }
        if c.peek() == Some(13) { c.point(13); }
    } else if c.peek() == Some(97) && c.t.get(c.p + 1).map(|t| t.0) != Some(330) {
        // "97 0": no fit points. (If it was the loop's own "97 0" instead,
        // taking it here is harmless: that group is optional.)
        c.p += 1;
    }
    if weights.len() != ctrl.len() { weights.clear(); }
    Edge::Spline { degree, knots, ctrl, weights, fit }
}
