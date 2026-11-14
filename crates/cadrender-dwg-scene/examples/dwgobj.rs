//! Print one decoded DWG object (Rust debug form) by hex handle.
//!
//! `cargo run --release -p cadrender-dwg-scene --example dwgobj -- FILE.dwg 1A2B`

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let d = cadrender_dwg::read_file(&a[1], &Default::default()).expect("dwg");
    let h = u64::from_str_radix(&a[2], 16).expect("hex handle");
    match d.get(h) {
        Some(o) => println!("{} {:#?}", d.type_name(o.type_code), o),
        None => println!("no object {h:X}"),
    }
}
