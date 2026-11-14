//! Tests on real drawings. They are not in the repository, so these tests
//! are ignored by default. Point them at a folder with your own .dwg files. Run them with:
//!   CADRENDER_DWG_SAMPLES=/path/to/dwg_folder cargo test -- --ignored

use cadrender_dwg::{read_bytes, read_file, DwgError, ReadOptions};

fn samples() -> Vec<std::path::PathBuf> {
    let Ok(dir) = std::env::var("CADRENDER_DWG_SAMPLES") else {
        return Vec::new();
    };
    let mut v: Vec<_> = std::fs::read_dir(dir)
        .map(|rd| rd.filter_map(|e| e.ok().map(|e| e.path())).collect())
        .unwrap_or_default();
    v.retain(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("dwg")));
    v.sort();
    v
}

#[test]
#[ignore]
fn all_supported_samples_decode_without_failures() {
    for p in samples() {
        match read_file(&p, &ReadOptions::default()) {
            Ok(d) => {
                assert!(d.failed.is_empty(), "{}: {} failed objects", p.display(), d.failed.len());
                for (name, s) in &d.stats {
                    assert_eq!(s.data_left, 0, "{}: {name} data stream not used exactly", p.display());
                    assert_eq!(s.handles_left, 0, "{}: {name} handles left", p.display());
                    assert_eq!(s.crc_bad, 0, "{}: {name} bad CRC", p.display());
                }
            }
            Err(DwgError::UnsupportedVersion(_)) => {}
            Err(e) => panic!("{}: {e}", p.display()),
        }
    }
}

/// Simple xorshift, so the test needs no extra crate.
fn next(seed: &mut u64) -> u64 {
    *seed ^= *seed << 13;
    *seed ^= *seed >> 7;
    *seed ^= *seed << 17;
    *seed
}

#[test]
#[ignore]
fn damaged_samples_never_panic() {
    let mut seed = 0x9E37_79B9_7F4A_7C15u64;
    for p in samples().into_iter().take(2) {
        let data = std::fs::read(&p).unwrap();
        for round in 0..40 {
            let mut d = data.clone();
            // flip a few random bytes, sometimes also truncate
            for _ in 0..(1 + round % 8) {
                let i = (next(&mut seed) as usize) % d.len();
                d[i] ^= (next(&mut seed) as u8) | 1;
            }
            if round % 5 == 0 {
                let cut = (next(&mut seed) as usize) % d.len();
                d.truncate(cut);
            }
            let _ = read_bytes(&d, &ReadOptions::default());
        }
    }
}
