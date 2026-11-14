//! dwgdump: read a DWG with cadrender-dwg and print a summary.
//!
//! Usage:
//!   dwgdump FILE.dwg            summary: header, timings, counts per type
//!   dwgdump FILE.dwg --fail N   also print the first N decode failures
//!   dwgdump FILE.dwg --json     one JSON object per decoded object (for
//!                                comparing with other readers)
//!   dwgdump FILE.dwg --serial   decode on one thread (for timing)

use std::io::Write;

use cadrender_dwg::{read_file, Drawing, ReadOptions};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let Some(path) = args.get(1) else {
        eprintln!("usage: dwgdump FILE.dwg [--fail N] [--json] [--serial]");
        std::process::exit(2);
    };
    let mut opts = ReadOptions::default();
    let mut nfail = 0usize;
    let mut json = false;
    let mut i = 2;
    while i < args.len() {
        match args[i].as_str() {
            "--serial" => opts.parallel = false,
            "--nocrc" => opts.check_crc = false,
            "--json" => json = true,
            "--fail" => {
                nfail = args.get(i + 1).and_then(|s| s.parse().ok()).unwrap_or(20);
                i += 1;
            }
            other => {
                eprintln!("unknown option {other}");
                std::process::exit(2);
            }
        }
        i += 1;
    }
    let d = match read_file(path, &opts) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("error: {e}");
            std::process::exit(1);
        }
    };
    if json {
        dump_json(&d);
        return;
    }
    summary(&d, nfail);
}

fn summary(d: &Drawing, nfail: usize) {
    let h = &d.header;
    println!("version {:?}  objects {}  failed {}", d.version, d.objects.len(), d.failed.len());
    println!(
        "timings: container {:?} sections {:?} header/classes/map {:?} objects {:?} (decode {:?}) total {:?}",
        d.timings.container, d.timings.sections, d.timings.header_classes_map, d.timings.objects, d.timings.objects_decode, d.timings.total
    );
    println!(
        "header: LTSCALE {} PSLTSCALE {} CELTSCALE {} INSUNITS {} LUNITS {} EXTMIN {:?} EXTMAX {:?} PEXTMIN {:?} PEXTMAX {:?}",
        h.ltscale, h.psltscale, h.celtscale, h.insunits, h.lunits, h.extmin, h.extmax, h.p_extmin, h.p_extmax
    );
    println!(
        "handles: model_space {:#x} paper_space {:#x} layouts dict {:#x} layer ctrl {:#x} handseed {:#x}",
        h.handles.model_space, h.handles.paper_space, h.handles.dict_layouts, h.handles.layer_control, h.handseed
    );
    println!("classes {}", d.classes.len());
    for w in &d.warnings {
        println!("warning: {w}");
    }
    println!("{:<34} {:>7} {:>7} {:>6} {:>7} {:>7} {:>6}", "type", "count", "decoded", "errors", "dataLeft", "hndLeft", "crcBad");
    for (name, s) in &d.stats {
        println!(
            "{:<34} {:>7} {:>7} {:>6} {:>7} {:>7} {:>6}",
            name, s.count, s.decoded, s.errors, s.data_left, s.handles_left, s.crc_bad
        );
    }
    for (h, name, e) in d.failed.iter().take(nfail) {
        println!("FAIL {h:#x} {name}: {e}");
    }
}

/// JSON string escaping.
fn js(s: &str) -> String {
    let mut o = String::with_capacity(s.len() + 2);
    o.push('"');
    for c in s.chars() {
        match c {
            '"' => o.push_str("\\\""),
            '\\' => o.push_str("\\\\"),
            '\n' => o.push_str("\\n"),
            '\r' => o.push_str("\\r"),
            '\t' => o.push_str("\\t"),
            c if (c as u32) < 0x20 => o.push_str(&format!("\\u{:04x}", c as u32)),
            c => o.push(c),
        }
    }
    o.push('"');
    o
}

fn num(v: f64) -> String {
    if v.is_finite() {
        format!("{v}")
    } else {
        "null".into()
    }
}

fn nums(v: &[f64]) -> String {
    let parts: Vec<String> = v.iter().map(|x| num(*x)).collect();
    format!("[{}]", parts.join(","))
}

fn color_str(c: &cadrender_dwg::Color) -> String {
    use cadrender_dwg::Color;
    match c {
        Color::ByLayer => "\"bylayer\"".into(),
        Color::ByBlock => "\"byblock\"".into(),
        Color::Index(i) => format!("{i}"),
        Color::Rgb(v) => format!("\"#{v:06x}\""),
        Color::None => "\"none\"".into(),
    }
}

/// One line of JSON per object, with the values the validation script
/// compares against ODA's DXF output: DXF type name, handle, layer name,
/// colour, a list of key numbers and a key string.
fn dump_json(d: &Drawing) {
    use cadrender_dwg::{EntityKind as K, ObjectKind};
    use std::collections::HashMap;
    let deg = |r: f64| r.to_degrees();
    let mut names: HashMap<u64, &str> = HashMap::new();
    for o in &d.objects {
        match &o.kind {
            ObjectKind::Layer(l) => {
                names.insert(o.handle, &l.name);
            }
            ObjectKind::BlockHeader(b) => {
                names.insert(o.handle, &b.name);
            }
            _ => {}
        }
    }
    // Full block names live in the BLOCK entity (anonymous block headers
    // may store only "*U" / "*D"), so map block header -> BLOCK entity name.
    let mut block_names: HashMap<u64, String> = HashMap::new();
    for o in &d.objects {
        if let ObjectKind::BlockHeader(b) = &o.kind {
            let full = d
                .get(b.block_entity)
                .and_then(|be| be.entity())
                .and_then(|e| match &e.kind {
                    K::Block { name } => Some(name.clone()),
                    _ => None,
                })
                .unwrap_or_else(|| b.name.clone());
            block_names.insert(o.handle, full);
        }
    }
    let out = std::io::stdout();
    let mut out = std::io::BufWriter::new(out.lock());
    for o in &d.objects {
        let raw = d.type_name(o.type_code);
        let Some(e) = o.entity() else {
            let s = match &o.kind {
                ObjectKind::Layer(l) => l.name.clone(),
                ObjectKind::Ltype(l) => l.name.clone(),
                ObjectKind::Style(st) => st.name.clone(),
                ObjectKind::BlockHeader(_) => block_names.get(&o.handle).cloned().unwrap_or_default(),
                ObjectKind::Layout(l) => l.name.clone(),
                ObjectKind::DimStyle(ds) => ds.name.clone(),
                ObjectKind::VPort(v) => v.name.clone(),
                _ => String::new(),
            };
            // (DXF group code, value) pairs, for the table checks of validate.py
            let groups: Vec<(i32, String)> = match &o.kind {
                ObjectKind::DimStyle(ds) => ds.raw.clone(),
                ObjectKind::MLeaderStyle(ms) => ms.raw.clone(),
                ObjectKind::XRecord(x) => x.items.iter().map(|(c, v)| (*c, format!("{v:?}"))).collect(),
                ObjectKind::VPort(v) => vec![(40, v.view_height.to_string()), (12, v.center[0].to_string()), (22, v.center[1].to_string())],
                _ => vec![],
            };
            let g = groups.iter().map(|(c, v)| format!("[{c},{}]", js(v))).collect::<Vec<_>>().join(",");
            let p: Vec<f64> = match &o.kind {
                ObjectKind::SortEnts(t) => vec![t.entries.len() as f64],
                ObjectKind::WipeoutVariables { frame } => vec![*frame as f64],
                ObjectKind::Layout(l) => {
                    vec![l.tab_order as f64, l.viewports.len() as f64, l.paper_width, l.paper_height]
                }
                ObjectKind::Ltype(l) => vec![l.pattern_length, l.dashes.len() as f64],
                ObjectKind::Dictionary(dd) => vec![dd.entries.len() as f64],
                _ => vec![],
            };
            let _ = writeln!(
                out,
                "{{\"h\":\"{:X}\",\"t\":{},\"raw\":{},\"owner\":\"{:X}\",\"s\":{},\"p\":{},\"g\":[{g}]}}",
                o.handle,
                js(raw),
                js(raw),
                o.owner,
                js(&s),
                nums(&p)
            );
            continue;
        };
        let (t, p, s): (&str, Vec<f64>, String) = match &e.kind {
            K::Line(l) => ("LINE", [l.start, l.end].concat(), String::new()),
            K::Circle(c) => ("CIRCLE", vec![c.center[0], c.center[1], c.center[2], c.radius], String::new()),
            K::Arc(a) => (
                "ARC",
                vec![a.center[0], a.center[1], a.center[2], a.radius, deg(a.start_angle), deg(a.end_angle)],
                String::new(),
            ),
            K::Point(pt) => ("POINT", pt.position.to_vec(), String::new()),
            K::Ellipse(el) => (
                "ELLIPSE",
                [el.center.to_vec(), el.major_axis.to_vec(), vec![el.axis_ratio, el.start_param, el.end_param]]
                    .concat(),
                String::new(),
            ),
            K::Solid(so) | K::Trace(so) => (
                if raw == "TRACE" { "TRACE" } else { "SOLID" },
                so.corners.iter().flat_map(|c| c.iter().copied()).collect(),
                String::new(),
            ),
            K::Text(tx) => {
                ("TEXT", vec![tx.insertion[0], tx.insertion[1], tx.height, deg(tx.rotation)], tx.value.clone())
            }
            K::Attrib(a) | K::AttDef(a) => (
                if raw == "ATTDEF" { "ATTDEF" } else { "ATTRIB" },
                vec![a.text.insertion[0], a.text.insertion[1], a.text.height, deg(a.text.rotation)],
                a.tag.clone(),
            ),
            K::MText(m) => (
                "MTEXT",
                vec![m.insertion[0], m.insertion[1], m.insertion[2], m.text_height, m.rect_width],
                m.value.clone(),
            ),
            K::Insert(i) => (
                "INSERT",
                [i.insertion.to_vec(), i.scale.to_vec(), vec![deg(i.rotation)]].concat(),
                block_names.get(&i.block_header).cloned().unwrap_or_default(),
            ),
            K::LwPolyline(lw) => {
                let mut p = vec![lw.vertices.len() as f64];
                if let (Some(a), Some(b)) = (lw.vertices.first(), lw.vertices.last()) {
                    p.extend_from_slice(&[a.point[0], a.point[1], b.point[0], b.point[1]]);
                }
                ("LWPOLYLINE", p, String::new())
            }
            K::Polyline2d(pl) => ("POLYLINE", vec![pl.flags as f64], String::new()),
            K::Vertex2d(v) => ("VERTEX", vec![v.point[0], v.point[1], v.bulge], String::new()),
            K::Hatch(h) => {
                ("HATCH", vec![h.paths.len() as f64, h.pattern_lines.len() as f64], h.pattern_name.clone())
            }
            K::Spline(sp) => (
                "SPLINE",
                vec![
                    sp.degree as f64,
                    sp.control_points.len() as f64,
                    sp.fit_points.len() as f64,
                    sp.knots.len() as f64,
                ],
                String::new(),
            ),
            K::Mesh(m) => ("MESH", vec![m.vertices.len() as f64, m.faces.len() as f64], String::new()),
            K::Helix(hx) => ("HELIX", vec![hx.start[0], hx.start[1], hx.start[2], hx.radius, hx.turns, hx.turn_height], String::new()),
            K::Wipeout(w) => (if raw == "IMAGE" { "IMAGE" } else { "WIPEOUT" }, w.insertion.to_vec(), String::new()),
            K::Dimension(dm) => ("DIMENSION", vec![], block_names.get(&dm.block).cloned().unwrap_or_default()),
            K::Viewport(v) => {
                ("VIEWPORT", vec![v.center[0], v.center[1], v.width, v.height, v.status as f64], String::new())
            }
            K::Block { name } => ("BLOCK", vec![], name.clone()),
            K::EndBlk => ("ENDBLK", vec![], String::new()),
            K::SeqEnd => ("SEQEND", vec![], String::new()),
            K::Other => (raw, vec![], String::new()),
            K::Acis(a) => (raw, vec![a.data.len() as f64], String::new()),
            K::Polyline3d(pl) => {
                let fl = 8 | (pl.closed_flags & 1) as i32 | if pl.spline_flags & 3 != 0 { 4 } else { 0 };
                ("POLYLINE", vec![fl as f64], String::new())
            }
            K::PolyMesh(pm) => ("POLYLINE", vec![pm.flags as f64], String::new()),
            K::Vertex3d(v) => ("VERTEX", vec![v.point[0], v.point[1], 0.0], String::new()),
            K::FaceRecord(_) => ("VERTEX", vec![0.0, 0.0, 0.0], String::new()),
            K::Leader(l) => {
                let mut p = vec![l.points.len() as f64];
                if let (Some(a), Some(b)) = (l.points.first(), l.points.last()) {
                    p.extend_from_slice(&[a[0], a[1], b[0], b[1]]);
                }
                ("LEADER", p, String::new())
            }
            K::XLine(x) => (if x.ray { "RAY" } else { "XLINE" }, x.base.to_vec(), String::new()),
            K::Tolerance(t) => ("TOLERANCE", t.insertion.to_vec(), t.text.clone()),
            K::Face3d(f) => ("3DFACE", f.corners.iter().flat_map(|c| c.iter().copied()).collect(), String::new()),
            K::MLine(m) => ("MLINE", vec![m.vertices.len() as f64, m.base[0], m.base[1]], String::new()),
            K::Table(t) => (
                "ACAD_TABLE",
                t.insert.insertion.to_vec(),
                block_names.get(&t.insert.block_header).cloned().unwrap_or_default(),
            ),
            K::MLeader(m) => {
                let c = &m.context;
                let lines: usize = c.roots.iter().map(|r| r.lines.len()).sum();
                let p = vec![c.roots.len() as f64, lines as f64, c.scale, c.content_base[0], c.content_base[1], c.text_height];
                ("MULTILEADER", p, c.text.as_ref().map(|t| t.label.clone()).unwrap_or_default())
            }
        };
        let layer = names.get(&e.common.layer).copied().unwrap_or("?");
        // Block record handle, to compare block references independent of
        // the anonymous block numbering (*U12) that DXF writers change.
        let bh = match &e.kind {
            K::Insert(i) => i.block_header,
            K::Table(t) => t.insert.block_header,
            K::Dimension(dm) => dm.block,
            K::Block { .. } => o.owner,
            _ => 0,
        };
        let _ = writeln!(
            out,
            "{{\"h\":\"{:X}\",\"bh\":\"{bh:X}\",\"t\":{},\"raw\":{},\"owner\":\"{:X}\",\"layer\":{},\"color\":{},\"lw\":{},\"s\":{},\"p\":{}}}",
            o.handle,
            js(t),
            js(raw),
            o.owner,
            js(layer),
            color_str(&e.common.color.color),
            e.common.lineweight_mm100(),
            js(&s),
            nums(&p)
        );
    }
}
