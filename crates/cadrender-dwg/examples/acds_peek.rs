//! List the sections of a DWG and save one (default AcDb:AcDsPrototype_1b) to a file.
//! `cargo run -p cadrender-dwg --example acds_peek -- FILE.dwg OUT [SECTION]`

fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let data = std::fs::read(&a[0]).expect("read");
    let c = cadrender_dwg::container::Container::parse(&data).expect("container");
    for s in &c.sections { println!("{}", s.name); }
    let name = a.get(2).map(String::as_str).unwrap_or("AcDb:AcDsPrototype_1b");
    let mut w = vec![];
    match c.section(name, &mut w).expect("section") {
        Some(b) => { std::fs::write(&a[1], &b).unwrap(); println!("{name}: {} bytes", b.len()); }
        None => println!("{name}: missing"),
    }
}
