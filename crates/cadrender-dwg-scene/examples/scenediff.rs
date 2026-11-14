//! Compare the Scene built from a DWG with the Scene built from ODA's DXF of
//! the same drawing. Entities are matched by handle.
//!
//! `cargo run --release -p cadrender-dwg-scene --example scenediff -- A.dwg A.dxf`

use cadrender_core::entity::{Entity, Kind};
use cadrender_core::scene::tables::{LinetypeId, LT_BYBLOCK, LT_BYLAYER, LT_SOLID};
use cadrender_core::text::{load_fonts, FontChoice};
use cadrender_core::Scene;
use std::collections::{BTreeMap, HashMap};

fn lt_name(s: &Scene, id: LinetypeId) -> String {
    match id { LT_BYLAYER => "BYLAYER".into(), LT_BYBLOCK => "BYBLOCK".into(), LT_SOLID => "CONTINUOUS".into(), i => s.linetypes.get(i as usize).map(|l| l.name.to_uppercase()).unwrap_or_default() }
}

/// handle -> (owner block handle, entity), attributes included.
fn index(s: &Scene) -> HashMap<u64, (u64, &Entity)> {
    let mut m = HashMap::new();
    for b in &s.blocks {
        for e in &b.ents {
            m.insert(e.props.handle, (b.handle, e));
            if let Kind::Insert(i) = &e.kind { for a in &i.attribs { m.insert(a.props.handle, (b.handle, a)); } }
        }
    }
    m
}

fn close(a: f64, b: f64, scale: f64) -> bool { (a - b).abs() <= 1e-6 * scale.max(1.0) }

/// What differs between two matched entities ("" = nothing).
fn diff(da: &Scene, a: &Entity, db: &Scene, b: &Entity) -> String {
    if a.kind.name() != b.kind.name() { return format!("kind {} vs {}", a.kind.name(), b.kind.name()); }
    let (pa, pb) = (&a.props, &b.props);
    // by handle: DXF writes control characters in names as ^X
    let (la, lb) = (&da.layers[pa.layer as usize], &db.layers[pb.layer as usize]);
    if la.handle != lb.handle || la.handle == 0 && la.name.to_uppercase() != lb.name.to_uppercase() { return format!("layer {:?} vs {:?}", la.name, lb.name); }
    if pa.color != pb.color { return format!("color {:?} vs {:?}", pa.color, pb.color); }
    if pa.lw != pb.lw { return format!("lw {} vs {}", pa.lw, pb.lw); }
    if lt_name(da, pa.ltype) != lt_name(db, pb.ltype) { return format!("ltype {} vs {}", lt_name(da, pa.ltype), lt_name(db, pb.ltype)); }
    if (pa.ltscale - pb.ltscale).abs() > 1e-5 * pa.ltscale.abs().max(1.0) { return "ltscale".into(); }
    if pa.alpha != pb.alpha { return "alpha".into(); }
    let (ba, bb) = (a.bbox, b.bbox);
    if ba.is_empty() != bb.is_empty() { return "bbox empty".into(); }
    let sc = ba.w().abs().max(ba.h().abs()).max(ba.x0.abs()).max(ba.y0.abs());
    let same = close(ba.x0, bb.x0, sc) && close(ba.y0, bb.y0, sc) && close(ba.x1, bb.x1, sc) && close(ba.y1, bb.y1, sc);
    if !ba.is_empty() && !same {
        return format!("bbox ({:.4},{:.4},{:.4},{:.4}) vs ({:.4},{:.4},{:.4},{:.4})", ba.x0, ba.y0, ba.x1, ba.y1, bb.x0, bb.y0, bb.x1, bb.y1);
    }
    match (&a.kind, &b.kind) {
        (Kind::Insert(x), Kind::Insert(y)) => {
            if da.blocks[x.block as usize].handle != db.blocks[y.block as usize].handle { return "insert block".into(); }
            if x.attribs.len() != y.attribs.len() { return format!("attribs {} vs {}", x.attribs.len(), y.attribs.len()); }
            if x.size() != y.size() { return "grid".into(); }
        }
        (Kind::Text(x), Kind::Text(y)) => {
            if x.glyphs.len() != y.glyphs.len() { return format!("glyphs {} vs {}", x.glyphs.len(), y.glyphs.len()); }
            if x.mask.is_some() != y.mask.is_some() { return "mask".into(); }
        }
        (Kind::Viewport(x), Kind::Viewport(y)) => {
            if (x.status, x.id, x.flags, x.top_view) != (y.status, y.id, y.flags, y.top_view) { return format!("vp status {:?} vs {:?}", (x.status, x.id, x.flags), (y.status, y.id, y.flags)); }
            if !close(x.twist, y.twist, 360.0) || !close(x.view_h, y.view_h, x.view_h) { return "vp view".into(); }
            let geo = |v: &cadrender_core::entity::viewport::ViewportEnt| [v.center[0], v.center[1], v.w, v.h, v.view_center[0], v.view_center[1], v.target[0], v.target[1]];
            if geo(x).iter().zip(geo(y)).any(|(p, q)| !close(*p, q, p.abs().max(1.0))) { return format!("vp geometry {:?} vs {:?}", geo(x), geo(y)); }
            if x.clip.as_ref().map(|c| c.len()) != y.clip.as_ref().map(|c| c.len()) { return "vp clip".into(); }
            // frozen layers by name; "<layer> @ <scale>" layers exist in the DWG only (SAVEFIDELITY files)
            let names = |s: &Scene, v: &[cadrender_core::scene::tables::LayerId]| -> std::collections::BTreeSet<String> { v.iter().map(|&l| s.layers[l as usize].name.to_uppercase()).filter(|n| !n.contains(" @ ")).collect() };
            let (fa, fb) = (names(da, &x.frozen_layers), names(db, &y.frozen_layers));
            if fa != fb { return format!("vp frozen: dwg only {:?}, dxf only {:?}", fa.difference(&fb).collect::<Vec<_>>(), fb.difference(&fa).collect::<Vec<_>>()); }
        }
        (Kind::Hatch(x), Kind::Hatch(y)) => {
            if x.solid != y.solid || x.pattern.len() != y.pattern.len() { return "hatch pattern".into(); }
            if x.pattern.iter().zip(&y.pattern).any(|(p, q)| !close(p.angle, q.angle, 1.0) || p.dashes.len() != q.dashes.len()) { return "hatch lines".into(); }
            let sc = |p: &cadrender_core::entity::hatch::PatLine| p.base.iter().chain(&p.offset).chain(&p.dashes).fold(1.0f64, |m, v| m.max(v.abs()));
            if let Some((p, q)) = x.pattern.iter().zip(&y.pattern).find(|(p, q)| p.base.iter().chain(&p.offset).chain(&p.dashes).zip(q.base.iter().chain(&q.offset).chain(&q.dashes)).any(|(u, v)| !close(*u, *v, sc(p)))) {
                return format!("hatch values {:?} vs {:?}", p, q);
            }
            if x.path.pts.len() != y.path.pts.len() || x.path.verbs != y.path.verbs { return format!("hatch path {} vs {} pts", x.path.pts.len(), y.path.pts.len()); }
            if x.gradient.is_some() != y.gradient.is_some() { return "hatch gradient".into(); }
        }
        _ => {}
    }
    String::new()
}

type Index<'a> = HashMap<u64, (u64, &'a Entity)>;

/// TREE=<handle>: an INSERT and, inside its block, every entity whose bbox
/// differs between the two scenes (or that is on one side only), recursively.
fn tree(a: &Scene, b: &Scene, ia: &Index, ib: &Index, h: u64, depth: usize) {
    let pad = "  ".repeat(depth);
    let (Some((_, x)), Some((_, y))) = (ia.get(&h), ib.get(&h)) else {
        println!("{pad}{h:X} only in {}", if ia.contains_key(&h) { "DWG" } else { "DXF" });
        return;
    };
    println!("{pad}{h:X} {} dwg {:?}\n{pad}   dxf {:?}  {}", x.kind.name(), x.bbox, y.bbox, diff(a, x, b, y));
    if depth > 6 { return; }
    let (Kind::Insert(p), Kind::Insert(q)) = (&x.kind, &y.kind) else { return };
    let (ba, bb) = (&a.blocks[p.block as usize], &b.blocks[q.block as usize]);
    let hb: HashMap<u64, &Entity> = bb.ents.iter().map(|e| (e.props.handle, e)).collect();
    for e in &ba.ents {
        match hb.get(&e.props.handle) {
            None => println!("{pad}  {:X} {} only in DWG layer {:?} {:?}", e.props.handle, e.kind.name(), a.layers[e.props.layer as usize].name, e.bbox),
            Some(f) if !diff(a, e, b, f).is_empty() => tree(a, b, ia, ib, e.props.handle, depth + 1),
            _ => {}
        }
    }
    let ha: std::collections::HashSet<u64> = ba.ents.iter().map(|e| e.props.handle).collect();
    for f in bb.ents.iter().filter(|f| !ha.contains(&f.props.handle)) {
        println!("{pad}  {:X} {} only in DXF {:?}", f.props.handle, f.kind.name(), f.bbox);
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let fonts = || load_fonts(&FontChoice::Liberation).expect("font");
    let t = std::time::Instant::now();
    let a = cadrender_dwg_scene::read_file(&args[1], fonts()).expect("dwg");
    let ta = t.elapsed();
    let t = std::time::Instant::now();
    let b = cadrender_dxf::read_file(&args[2], fonts()).expect("dxf");
    println!("read: dwg {:?}  dxf {:?}", ta, t.elapsed());
    println!("entities: dwg {}  dxf {}", a.entity_count(), b.entity_count());
    let (ia, ib) = (index(&a), index(&b));
    if let Some(h) = std::env::var("DUMP").ok().and_then(|h| u64::from_str_radix(&h, 16).ok()) {
        println!("DWG: {:?}\nDXF: {:?}", ia.get(&h).map(|e| &e.1.kind), ib.get(&h).map(|e| &e.1.kind));
        return;
    }
    // AT=x,y: model space entities (top level) whose bbox holds that point, both sides
    if let Some(p) = std::env::var("AT").ok().map(|s| s.split(',').filter_map(|v| v.parse::<f64>().ok()).collect::<Vec<f64>>()).filter(|p| p.len() == 2) {
        for (side, s) in [("DWG", &a), ("DXF", &b)] {
            for e in &s.blocks[s.model as usize].ents {
                let bb = e.bbox;
                if bb.x0 - 0.05 <= p[0] && p[0] <= bb.x1 + 0.05 && bb.y0 - 0.05 <= p[1] && p[1] <= bb.y1 + 0.05 {
                    println!("{side} {:X} {} layer {:?} {:?}", e.props.handle, e.kind.name(), s.layers[e.props.layer as usize].name, bb);
                }
            }
        }
        return;
    }
    if let Some(h) = std::env::var("TREE").ok().and_then(|h| u64::from_str_radix(&h, 16).ok()) {
        tree(&a, &b, &ia, &ib, h, 0);
        return;
    }
    let mut cats: BTreeMap<String, (usize, Vec<String>)> = BTreeMap::new();
    let mut add = |cat: String, ex: String| { let c = cats.entry(cat).or_default(); c.0 += 1; if c.1.len() < std::env::var("EX").ok().and_then(|v| v.parse().ok()).unwrap_or(3) { c.1.push(ex); } };
    // TAB=<name>: only entities owned by that tab's block (handle of the DWG side's block)
    let tab_block = std::env::var("TAB").ok().and_then(|t| a.layouts.iter().find(|l| l.name == t).map(|l| a.blocks[l.block as usize].handle));
    if let Some(tb) = tab_block { println!("tab block {tb:X}"); }
    // with TAB, model space entities seen through one of the tab's viewports count too
    let windows: Vec<cadrender_core::geom::BBox> = std::env::var("TAB").ok().and_then(|t| a.layouts.iter().find(|l| l.name == t))
        .map(|l| a.viewports(l).map(|v| v.visible_rect().transformed(&v.matrix().inverse())).collect()).unwrap_or_default();
    let model_h = a.blocks[a.model as usize].handle;
    let in_tab = |owner: u64, e: &Entity| tab_block.is_none_or(|tb| tb == owner || owner == model_h && windows.iter().any(|w| !w.intersect(&e.bbox).is_empty()));
    for (h, (owner, e)) in &ia {
        if !in_tab(*owner, e) { continue; }
        match ib.get(h) {
            None => add(format!("only in DWG: {}", e.kind.name()), format!("{h:X}")),
            Some((_, f)) => { let d = diff(&a, e, &b, f); if !d.is_empty() { add(format!("differs: {} {}", e.kind.name(), d.split(' ').next().unwrap_or("")), format!("{h:X} {d}")); } }
        }
    }
    for (h, (_, e)) in &ib { if tab_block.is_none() && !ia.contains_key(h) { add(format!("only in DXF: {}", e.kind.name()), format!("{h:X}")); } }
    for (c, (n, ex)) in &cats { println!("{n:>8}  {c}   e.g. {}", ex.join(" | ")); }
    // tables and tabs
    let la: Vec<_> = a.layouts.iter().map(|l| (&l.name, l.viewports.len(), &l.paper)).collect();
    let lb: Vec<_> = b.layouts.iter().map(|l| (&l.name, l.viewports.len(), &l.paper)).collect();
    println!("layouts equal: {}", la == lb);
    for (x, y) in la.iter().zip(&lb) { if x != y { println!("  layout {:?} vs {:?}", x, y); break; } }
    println!("layers {} vs {}, linetypes {} vs {}, styles {} vs {}, dimstyles {} vs {}, mleaderstyles {} vs {}",
        a.layers.len(), b.layers.len(), a.linetypes.len(), b.linetypes.len(), a.text_styles.len(), b.text_styles.len(), a.dim_styles.len(), b.dim_styles.len(), a.mleader_styles.len(), b.mleader_styles.len());
    let mut bad_layers = vec![];
    for la in &a.layers {
        let Some(lb) = b.layers.iter().find(|l| l.handle == la.handle && l.handle != 0) else { continue };
        if (if la.true_color.is_some() { 0 } else { la.color }, la.true_color, la.lw, la.alpha, la.off, la.frozen, la.plot, la.vp_overrides.len()) != (if lb.true_color.is_some() { 0 } else { lb.color }, lb.true_color, lb.lw, lb.alpha, lb.off, lb.frozen, lb.plot, lb.vp_overrides.len())
            || lt_name(&a, la.ltype) != lt_name(&b, lb.ltype) {
            if bad_layers.len() < 2 { println!("  layer {:?}: dwg {:?} {}\n                 dxf {:?} {}", la.name, (la.color, la.true_color, la.lw, la.alpha, la.off, la.frozen, la.plot), lt_name(&a, la.ltype), (lb.color, lb.true_color, lb.lw, lb.alpha, lb.off, lb.frozen, lb.plot), lt_name(&b, lb.ltype)); }
            bad_layers.push(la.name.clone());
        }
    }
    println!("layers that differ: {} {:?}", bad_layers.len(), &bad_layers[..bad_layers.len().min(5)]);
    let bad_lt: Vec<_> = a.linetypes.iter().filter(|x| !b.linetypes.iter().any(|y| y.name.eq_ignore_ascii_case(&x.name) && y.pattern.len() == x.pattern.len() && y.parts.len() == x.parts.len() && y.complex == x.complex)).map(|x| x.name.clone()).collect();
    println!("linetypes that differ: {:?}", bad_lt);
    for x in &a.linetypes {
        if let Some(y) = b.linetypes.iter().find(|y| y.name.eq_ignore_ascii_case(&x.name)) {
            if x.pattern.len() == y.pattern.len() && x.pattern.iter().zip(&y.pattern).any(|(p, q)| (p - q).abs() > 1e-12) { println!("  linetype {} values {:?} vs {:?}", x.name, x.pattern, y.pattern); }
        }
    }
    println!("header dwg {:?}\nheader dxf {:?}", a.header, b.header);
    println!("skipped dwg {:?}\nskipped dxf {:?}", a.stats.skipped, b.stats.skipped);
}
