//! Test pictures (made with Pillow): red_blue.* is 8 x 4, left half red,
//! right half blue; gray.jpg is 3 x 3 grey 128.

use super::*;

fn data() -> PathBuf { Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data") }

fn px(p: &Picture, x: u32, y: u32) -> [u8; 4] { let i = ((y * p.w + x) * 4) as usize; p.rgba[i..i + 4].try_into().unwrap() }

#[test]
fn png_and_jpeg() {
    for (f, mime) in [("red_blue.png", "image/png"), ("red_blue.jpg", "image/jpeg")] {
        let p = load_file(&data().join(f)).unwrap();
        assert_eq!((p.w, p.h, p.mime, p.gray), (8, 4, mime, false), "{f}");
        let (l, r) = (px(&p, 1, 1), px(&p, 6, 2));
        assert!(l[0] > 200 && l[2] < 60 && r[2] > 200 && r[0] < 60, "{f}: {l:?} {r:?}");
        assert_eq!(p.encoded, std::fs::read(data().join(f)).unwrap());
    }
    let g = load_file(&data().join("gray.jpg")).unwrap();
    assert!(g.gray && (px(&g, 1, 1)[0] as i32 - 128).abs() <= 2);
    assert!(decode(b"GIF89a".to_vec()).is_err());
}

#[test]
fn resolve_tries_drawing_folder() {
    let d = data();
    // relative Windows path, other case, and a foreign absolute path
    // (macOS file systems ignore case themselves, Linux ones need find_nocase)
    let p = resolve(".\\RED_BLUE.PNG", Some(&d)).unwrap();
    assert_eq!(p.file_name().unwrap().to_string_lossy().to_lowercase(), "red_blue.png");
    assert!(p.is_file());
    assert_eq!(resolve("C:\\Users\\x\\IMMAGINI\\red_blue.jpg", Some(&d)), Some(d.join("red_blue.jpg")));
    assert_eq!(resolve("missing.png", Some(&d)), None);
    assert_eq!(resolve("", Some(&d)), None);
    let abs = d.join("gray.jpg");
    assert_eq!(resolve(abs.to_str().unwrap(), None), Some(abs));
}
