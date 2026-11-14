//! Show how font names resolve on this machine.
//!
//! cargo run --example resolve -- romans.shx arial.ttf "family:Swis721 Lt BT"
//! With no arguments it checks the fonts found in the sample drawings.

use cadrender_shx::{FontResolver, FontSource};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let names: Vec<String> = if args.is_empty() {
        [
            "romans.shx",
            "arial.ttf",
            "swissl.ttf",
            "arialn.ttf",
            "gothic.ttf",
            "simplex.shx",
            "stylu.ttf",
            "verdana.ttf",
            "romand.shx",
            "isocpeur.ttf",
            "txt.shx",
            "isocp",
            "swiss.ttf",
            "gothicb.ttf",
            "arialbd.ttf",
            "arialni.ttf",
            "ltypeshp.shx",
            "family:@Arial Unicode MS",
            "family:Arial",
            "family:Arial Narrow",
            "family:Century Gothic",
            "family:Symbol",
            "family:Verdana",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect()
    } else {
        args
    };
    let t = std::time::Instant::now();
    let r = FontResolver::system();
    println!("scanned {} font files in {:?}\n", r.len(), t.elapsed());
    for n in names {
        let res = match n.strip_prefix("family:") {
            Some(f) => r.resolve_family(f, false, false),
            None => r.resolve(&n, None),
        };
        let src = match &res.source {
            FontSource::Shx(p) => format!("SHX {}", p.display()),
            FontSource::TrueType { path, index } => format!("TTF {} #{index}", path.display()),
            FontSource::Builtin(b) => b.name().to_string(),
        };
        println!(
            "{:<28} {:<9} w={:<5} {}",
            n,
            format!("{:?}", res.quality),
            res.width_scale,
            src
        );
    }
}
