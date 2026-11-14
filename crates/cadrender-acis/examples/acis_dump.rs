//! Print the records and the outline summary of a SAT or SAB file.
//! `cargo run -p cadrender-acis --example acis_dump -- FILE [--records]`

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(path) = args.first() else { eprintln!("usage: acis_dump FILE [--records]"); std::process::exit(2) };
    let data = std::fs::read(path).expect("read");
    if args.iter().any(|a| a == "--records") {
        let f = if data.starts_with(b"ACIS BinaryFile") || data.starts_with(b"ASM BinaryFile") {
            cadrender_acis::tokens::parse_sab(&data)
        } else {
            cadrender_acis::tokens::parse_sat(&String::from_utf8_lossy(&data))
        };
        match f {
            Some(f) => {
                println!("version {}", f.version);
                for (i, r) in f.records.iter().enumerate() { println!("{i:4} {} {:?}", r.name, r.toks); }
            }
            None => println!("cannot parse"),
        }
    }
    match cadrender_acis::outline(&data) {
        Some(o) => println!("edges {} skipped {} bbox {:?}", o.edges, o.skipped, o.path.bbox()),
        None => println!("no outline"),
    }
}
