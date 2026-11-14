//! `cadrender` command line tool:
//!
//! ```text
//! cadrender sheets <file.dxf|dwg>
//! cadrender render <file.dxf|dwg> --sheet <NAME> --width 2000 -o out.png
//! cadrender render <file.dxf|dwg> --all --width 2000 --out-dir DIR [--threads N] [--svg]
//! ```
//! `render` prints a JSON summary with per-sheet milliseconds to stdout.

mod json;

use cadrender::{load_fonts, sheet_file_name, Document, FontChoice, RenderOptions, SvgConfig, SvgText};
use cadrender_core::{ColorPolicy, Framing, LineweightPolicy, Lod};
use json::Json;
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Instant;

const USAGE: &str = "usage:
  cadrender sheets <file.dxf|dwg>
  cadrender render <file.dxf|dwg> --sheet <NAME> [--width 2000] -o out.png|out.svg
  cadrender render <file.dxf|dwg> --all [--width 2000] --out-dir DIR [--threads N] [--svg]

render options:
  --sheets A,B,...       with --out-dir: only these sheets
  --preset preview|none  option set (default: preview = grey look, no Defpoints)
  --color mono|true      colour policy
  --lineweight preview|paper|hairline
  --dpi N                dpi for the preview lineweight rule (default 150)
  --frame extents|paper  picture frame: drawing extents + 5 % (default) or layout paper
  --no-clean             keep everything (no clean rules)
  --full-detail          no level-of-detail shortcuts (also --no-lod)
  --no-draw-order        ignore the SORTENTS draw order
  --no-masks             do not draw MTEXT background masks
  --no-wipeout-frames    never draw WIPEOUT frames (default: as the file says)
  --font auto|liberation|FILE.ttf
                         font for all text (default auto: Arial if installed,
                         else the built-in Liberation Sans)
  --threads N            worker threads (default: all cores)

SVG options (--svg writes an .svg next to each .png; -o x.svg writes SVG only):
  --svg-only             with --out-dir: write only the .svg files
  --svg-glyphs           text as glyph outlines (exact look, not selectable)
  --svg-precision N      decimals of coordinates (default: from the size)
  --svg-lod-zoom Z       weaker level of detail for SVG (default 4; 1 = as PNG)";

struct Args {
    cmd: String,
    input: Option<PathBuf>,
    sheet: Option<String>,
    sheets: Option<Vec<String>>,
    all: bool,
    out: Option<PathBuf>,
    out_dir: Option<PathBuf>,
    threads: usize,
    svg: bool,
    svg_only: bool,
    svg_cfg: SvgConfig,
    font: FontChoice,
    opts: RenderOptions,
}

fn parse_args(argv: &[String]) -> Result<Args, String> {
    let mut it = argv.iter().skip(1);
    let cmd = it.next().ok_or("missing command")?.clone();
    if cmd == "-h" || cmd == "--help" || cmd == "help" { return Err(String::new()); }
    let mut a = Args { cmd, input: None, sheet: None, sheets: None, all: false, out: None, out_dir: None, threads: 0, svg: false, svg_only: false, svg_cfg: SvgConfig::default(), font: FontChoice::Auto, opts: RenderOptions::preset("preview").expect("preview preset") };
    // first pass for the preset, so later flags change it
    let rest: Vec<String> = it.cloned().collect();
    if let Some(i) = rest.iter().position(|x| x == "--preset") {
        let n = rest.get(i + 1).ok_or("--preset needs a value")?;
        a.opts = RenderOptions::preset(n).ok_or_else(|| format!("unknown preset {n:?} (known: {})", RenderOptions::PRESETS.join(", ")))?;
    }
    let mut i = 0;
    while i < rest.len() {
        let flag = rest[i].as_str();
        let mut val = || -> Result<String, String> { i += 1; rest.get(i).cloned().ok_or_else(|| format!("{flag} needs a value")) };
        match flag {
            "--sheet" => a.sheet = Some(val()?),
            "--sheets" => a.sheets = Some(val()?.split(',').map(str::to_string).collect()),
            "--all" => a.all = true,
            "-o" | "--out" => a.out = Some(val()?.into()),
            "--out-dir" => a.out_dir = Some(val()?.into()),
            "--width" => a.opts.width = val()?.parse().map_err(|_| "--width must be a number")?,
            "--dpi" => a.opts.dpi = val()?.parse().map_err(|_| "--dpi must be a number")?,
            "--threads" => a.threads = val()?.parse().map_err(|_| "--threads must be a number")?,
            "--svg" => a.svg = true,
            "--svg-only" => { a.svg = true; a.svg_only = true; }
            "--svg-glyphs" => a.svg_cfg.text = SvgText::Paths,
            "--svg-precision" => a.svg_cfg.precision = Some(val()?.parse().map_err(|_| "--svg-precision must be 0..6")?),
            "--svg-lod-zoom" => a.svg_cfg.lod_zoom = val()?.parse().map_err(|_| "--svg-lod-zoom must be a number")?,
            "--font" => a.font = FontChoice::parse(&val()?),
            "--preset" => { val()?; }
            "--color" => a.opts.color = match val()?.as_str() { "mono" | "monochrome" => ColorPolicy::MonochromeLight, "true" | "truecolor" => ColorPolicy::TrueColor, x => return Err(format!("unknown colour policy {x:?}")) },
            "--lineweight" => a.opts.lineweight.policy = match val()?.as_str() { "preview" => LineweightPolicy::Preview, "paper" => LineweightPolicy::Paper, "hairline" => LineweightPolicy::Hairline, x => return Err(format!("unknown lineweight policy {x:?}")) },
            "--frame" => a.opts.framing = match val()?.as_str() { "extents" => Framing::Extents, "paper" => Framing::Paper, x => return Err(format!("unknown frame {x:?}")) },
            "--no-clean" => a.opts.clean = Default::default(),
            "--no-lod" | "--full-detail" => a.opts.lod = Lod::full(),
            "--no-draw-order" => a.opts.draw_order = false,
            "--no-masks" => a.opts.text_masks = false,
            "--no-wipeout-frames" => a.opts.wipeout_frames = false,
            x if x.starts_with('-') => return Err(format!("unknown option {x}")),
            x => { if a.input.is_some() { return Err(format!("unexpected argument {x}")); } a.input = Some(x.into()); }
        }
        i += 1;
    }
    Ok(a)
}

fn main() -> ExitCode {
    let argv: Vec<String> = std::env::args().collect();
    let a = match parse_args(&argv) {
        Ok(a) => a,
        Err(e) => { if !e.is_empty() { eprintln!("error: {e}"); } eprintln!("{USAGE}"); return ExitCode::from(2); }
    };
    let Some(input) = a.input.clone() else { eprintln!("error: missing input file\n{USAGE}"); return ExitCode::from(2) };
    match a.cmd.as_str() {
        "sheets" => cmd_sheets(&input, &a),
        "render" => cmd_render(&input, a),
        c => { eprintln!("error: unknown command {c:?}\n{USAGE}"); ExitCode::from(2) }
    }
}

fn open(input: &PathBuf, a: &Args) -> Result<Document, ExitCode> {
    let fonts = match load_fonts(&a.font) {
        Ok(f) => f,
        Err(e) => { eprintln!("error: font: {e}"); return Err(ExitCode::from(2)); }
    };
    Document::open(input, fonts).map_err(|e| { eprintln!("error: {}: {e}", input.display()); ExitCode::from(1) })
}

fn cmd_sheets(input: &PathBuf, a: &Args) -> ExitCode {
    // sheet names need no fonts; skip loading them
    let doc = match Document::open(input, cadrender::Fonts::new()) { Ok(d) => d, Err(e) => { eprintln!("error: {}: {e}", input.display()); return ExitCode::from(1); } };
    let _ = a;
    for s in doc.sheets() { println!("{s}"); }
    ExitCode::SUCCESS
}

fn cmd_render(input: &PathBuf, a: Args) -> ExitCode {
    let single = match (&a.sheet, a.all) {
        (Some(_), false) if a.out.is_some() => true,
        (None, true) if a.out_dir.is_some() => false,
        _ => { eprintln!("error: use --sheet NAME -o FILE, or --all --out-dir DIR\n{USAGE}"); return ExitCode::from(2); }
    };
    if a.threads > 0 {
        if let Err(e) = rayon::ThreadPoolBuilder::new().num_threads(a.threads).build_global() { eprintln!("warning: --threads: {e}"); }
    }
    let t0 = Instant::now();
    let doc = match open(input, &a) { Ok(d) => d, Err(c) => return c };
    let t1 = Instant::now();
    let prep = doc.prepare(&a.opts);
    let prepare_ms = t1.elapsed().as_secs_f64() * 1e3;
    let stem = input.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| "drawing".into());

    let names: Vec<String> = if single { vec![a.sheet.clone().unwrap_or_default()] } else {
        let all: Vec<String> = doc.sheets().iter().map(|s| s.to_string()).collect();
        match &a.sheets { Some(want) => all.into_iter().filter(|s| want.contains(s)).collect(), None => all }
    };
    if let Some(d) = &a.out_dir {
        if let Err(e) = std::fs::create_dir_all(d) { eprintln!("error: {}: {e}", d.display()); return ExitCode::from(1); }
    }
    let t2 = Instant::now();
    let single_svg = single && a.out.as_ref().is_some_and(|o| o.extension().is_some_and(|e| e.eq_ignore_ascii_case("svg")));
    let want_png = !a.svg_only && !single_svg;
    let want_svg = a.svg || single_svg;
    let out_path = |name: &str, ext: &str| -> PathBuf {
        if single {
            let o = a.out.clone().unwrap_or_default();
            if single_svg || ext == "png" { o } else { o.with_extension(ext) }
        } else { a.out_dir.clone().unwrap_or_default().join(sheet_file_name(&stem, name, ext)) }
    };
    let stats_json = |s: cadrender::Stats| {
        let mut st = Json::obj();
        for (k, v) in [("visited", s.visited), ("drawn", s.drawn), ("culled", s.culled), ("tiny", s.tiny), ("hatch_lines", s.hatch_lines), ("hatch_dense", s.hatch_dense), ("glyphs", s.glyphs), ("flushes", s.flushes)] { st.num(k, v as f64); }
        st
    };
    let do_sheet = |name: &str| -> (bool, Json) {
        let mut j = Json::obj();
        j.str("sheet", name);
        if want_png {
            let img = match doc.render_png_isolated(name, &a.opts, &prep) { Ok(i) => i, Err(e) => { j.bool("ok", false); j.str("error", &e); return (false, j); } };
            let path = out_path(name, "png");
            let tw = Instant::now();
            if let Err(e) = std::fs::write(&path, &img.png) { j.bool("ok", false); j.str("error", &format!("{}: {e}", path.display())); return (false, j); }
            let write_ms = tw.elapsed().as_secs_f64() * 1e3;
            j.bool("ok", true);
            j.str("file", &path.to_string_lossy());
            j.num("width", img.width as f64); j.num("height", img.height as f64);
            j.bool("empty", img.empty);
            j.num("viewports", img.viewports as f64);
            j.ms("ms", img.render_ms + img.encode_ms + write_ms);
            j.ms("render_ms", img.render_ms); j.ms("encode_ms", img.encode_ms); j.ms("write_ms", write_ms);
            j.raw("stats", stats_json(img.stats));
            j.strs("warnings", &img.warnings);
        }
        if want_svg {
            let img = match doc.render_svg_isolated(name, &a.opts, &prep, &a.svg_cfg) { Ok(i) => i, Err(e) => { j.bool("ok", false); j.str("error", &format!("svg: {e}")); return (false, j); } };
            let path = out_path(name, "svg");
            let tw = Instant::now();
            if let Err(e) = std::fs::write(&path, &img.svg) { j.bool("ok", false); j.str("error", &format!("{}: {e}", path.display())); return (false, j); }
            let write_ms = tw.elapsed().as_secs_f64() * 1e3;
            if !want_png {
                j.bool("ok", true);
                j.str("file", &path.to_string_lossy());
                j.num("width", img.width as f64); j.num("height", img.height as f64);
                j.bool("empty", img.empty);
                j.num("viewports", img.viewports as f64);
                j.ms("ms", img.render_ms + write_ms);
                j.raw("stats", stats_json(img.stats));
                j.strs("warnings", &img.warnings);
            }
            j.str("svg_file", &path.to_string_lossy());
            j.num("svg_bytes", img.svg.len() as f64);
            j.ms("svg_ms", img.render_ms + write_ms);
            j.ms("svg_render_ms", img.render_ms);
            j.ms("svg_write_ms", write_ms);
        }
        (true, j)
    };
    let rows: Vec<(bool, Json)> = { use rayon::prelude::*; names.par_iter().map(|n| do_sheet(n)).collect() };
    let render_ms = t2.elapsed().as_secs_f64() * 1e3;
    let failed = rows.iter().filter(|r| !r.0).count();
    let sc = &doc.scene;
    let mut out = Json::obj();
    out.str("file", &input.to_string_lossy());
    out.num("size_mb", (doc.size_bytes as f64 / 1e6 * 10.0).round() / 10.0);
    out.str("version", &sc.header.version);
    out.str("font", sc.fonts.faces.first().map(|f| f.name.as_str()).unwrap_or(""));
    out.num("threads", rayon::current_num_threads() as f64);
    out.ms("parse_ms", doc.parse_ms);
    out.ms("prepare_ms", prepare_ms);
    out.ms("render_wall_ms", render_ms);
    out.ms("total_ms", t0.elapsed().as_secs_f64() * 1e3);
    out.num("sheets_total", rows.len() as f64);
    out.num("sheets_failed", failed as f64);
    out.num("entities", sc.entity_count() as f64);
    out.num("blocks", sc.blocks.len() as f64);
    out.num("layers", sc.layers.len() as f64);
    out.num("model_expanded", sc.model_block().expanded as f64);
    out.map("entity_counts", &sc.stats.counts);
    out.map("skipped_types", &sc.stats.skipped);
    out.strs("reader_warnings", &sc.stats.warnings);
    out.arr("sheets", rows.into_iter().map(|r| r.1).collect());
    println!("{}", out.finish());
    if single && failed > 0 { ExitCode::from(1) } else { ExitCode::SUCCESS }
}
