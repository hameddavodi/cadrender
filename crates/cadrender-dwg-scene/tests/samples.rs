//! DWG -> Scene against ODA's DXF of the same drawing. Real drawings are
//! not in the repository, so these tests are ignored by default. Point them
//! at a folder with your own .dwg files. Run them with:
//!   CADRENDER_DWG_SAMPLES=/path/to/dwg_folder cargo test -p cadrender-dwg-scene -- --ignored
//! The DXF files are looked up in `../dxf_oda/<name>.dxf` next to that folder.

use cadrender_core::text::Fonts;
use std::path::PathBuf;

/// (DWG, ODA DXF) pairs.
fn pairs() -> Vec<(PathBuf, PathBuf)> {
    let Ok(dir) = std::env::var("CADRENDER_DWG_SAMPLES") else { return vec![] };
    let dir = PathBuf::from(dir);
    let mut v: Vec<(PathBuf, PathBuf)> = std::fs::read_dir(&dir)
        .map(|r| r.filter_map(|e| e.ok().map(|e| e.path())).collect::<Vec<_>>())
        .unwrap_or_default()
        .into_iter()
        .filter(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("dwg")))
        .map(|p| {
            let dxf = dir.join("../dxf_oda").join(p.with_extension("dxf").file_name().unwrap_or_default());
            (p, dxf)
        })
        .filter(|(_, d)| d.exists())
        .collect();
    v.sort();
    v
}

fn fonts() -> Fonts { Fonts::default() }

#[test]
#[ignore]
fn dwg_scene_has_the_same_tabs_and_tables_as_the_dxf() {
    let pairs = pairs();
    assert!(!pairs.is_empty(), "set CADRENDER_DWG_SAMPLES to the folder with the .dwg files");
    for (dwg, dxf) in pairs {
        let a = cadrender_dwg_scene::read_file(&dwg, fonts()).unwrap_or_else(|e| panic!("{}: {e}", dwg.display()));
        let b = cadrender_dxf::read_file(&dxf, fonts()).unwrap_or_else(|e| panic!("{}: {e}", dxf.display()));
        let tabs = |s: &cadrender_core::scene::Scene| s.layouts.iter().map(|l| l.name.clone()).collect::<Vec<_>>();
        assert_eq!(tabs(&a), tabs(&b), "{}: tab list", dwg.display());
        assert_eq!(a.linetypes.len(), b.linetypes.len(), "{}: linetypes", dwg.display());
        assert_eq!(a.text_styles.len(), b.text_styles.len(), "{}: text styles", dwg.display());
        assert_eq!(a.dim_styles.len(), b.dim_styles.len(), "{}: dimstyles", dwg.display());
        // SAVEFIDELITY files keep extra "X @ scale" layers that ODA drops
        assert!(a.layers.len() >= b.layers.len(), "{}: layers", dwg.display());
    }
}
