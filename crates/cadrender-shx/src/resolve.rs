//! Font resolution: map a text style's font name to a font file or a
//! built-in fallback.
//!
//! A STYLE table entry names a font file (group code 3, for example
//! `romans.shx` or `arial.ttf`), sometimes a TrueType family name in its
//! ACAD xdata (for example "Swis721 Lt BT"), and maybe a big font (code 4).
//! MTEXT can switch fonts inline with `\Ffile;` or `\fFamily|b0|i0;`.
//!
//! Order of the search:
//! 1. The exact file name in the search folders (case-insensitive).
//! 2. The same font under another file name (macOS calls arial.ttf
//!    "Arial.ttf", arialn.ttf "Arial Narrow.ttf", ...).
//! 3. A similar free font from the fallback table, with a width hint.
//! 4. A built-in Hershey stroke font (always there).

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::builtin::BuiltinFont;

/// Where the glyphs come from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FontSource {
    /// An SHX file on disk (load it with [`crate::ShxFont::from_file`]).
    Shx(PathBuf),
    /// A TrueType / OpenType file (`index` is the face in a .ttc collection).
    TrueType { path: PathBuf, index: u32 },
    /// A built-in stroke font.
    Builtin(BuiltinFont),
}

/// How good the match is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum MatchQuality {
    /// The requested file itself.
    Exact,
    /// The same font under another file name.
    SameFont,
    /// A different font that looks similar (metric-compatible when possible).
    Similar,
    /// Last resort: a built-in stroke font or the default sans font.
    Fallback,
}

/// Result of a resolution.
#[derive(Debug, Clone, PartialEq)]
pub struct Resolved {
    pub source: FontSource,
    pub quality: MatchQuality,
    /// Multiply the style width factor by this to get closer to the
    /// requested font's widths (for example 0.82 when Arial Narrow is drawn
    /// with a normal-width font). 1.0 when the match is exact.
    pub width_scale: f32,
    /// What was asked for (normalized, for logs).
    pub requested: String,
}

impl Resolved {
    pub fn is_shx(&self) -> bool {
        matches!(self.source, FontSource::Shx(_) | FontSource::Builtin(_))
    }
    pub fn is_substitute(&self) -> bool {
        self.quality > MatchQuality::Exact
    }
}

/// One row of the fallback table.
pub struct FallbackRow {
    /// Lower-case keys: file names, file stems, family names.
    pub keys: &'static [&'static str],
    /// File names of the same font.
    pub same: &'static [&'static str],
    /// File names of similar fonts with a width hint.
    pub similar: &'static [(&'static str, f32)],
    /// Built-in stroke font to use when nothing above is found.
    pub builtin: Option<BuiltinFont>,
}

const LIB_SANS: (&str, f32) = ("LiberationSans-Regular.ttf", 1.0);
const ARIMO: (&str, f32) = ("Arimo-Regular.ttf", 1.0);

/// The fallback table. Licences of the suggested free fonts are listed in
/// the crate README. Only fonts that are on the machine are used.
pub static FALLBACKS: &[FallbackRow] = &[
    // --- SHX ----------------------------------------------------------
    FallbackRow {
        keys: &[
            "romans.shx",
            "simplex.shx",
            "txt.shx",
            "isocp.shx",
            "isoct.shx",
            "isocp2.shx",
            "isoct2.shx",
            "isocp3.shx",
            "isoct3.shx",
            "monotxt.shx",
            "italic.shx",
            "scripts.shx",
            "romans",
            "simplex",
            "txt",
            "isocp",
            "isoct",
            "monotxt",
        ],
        same: &[],
        similar: &[],
        builtin: Some(BuiltinFont::Simplex),
    },
    FallbackRow {
        keys: &[
            "romand.shx",
            "romanc.shx",
            "romant.shx",
            "complex.shx",
            "italicc.shx",
            "italict.shx",
            "scriptc.shx",
            "romand",
            "romanc",
            "romant",
            "complex",
        ],
        same: &[],
        similar: &[],
        builtin: Some(BuiltinFont::Duplex),
    },
    // --- TrueType: Arial family (Liberation Sans / Arimo are metric-compatible)
    FallbackRow {
        keys: &["arial.ttf", "arial"],
        same: &["arial.ttf", "Arial.ttf"],
        similar: &[
            LIB_SANS,
            ARIMO,
            ("Helvetica.ttc", 1.0),
            ("DejaVuSans.ttf", 0.9),
        ],
        builtin: None,
    },
    // Arial Unicode MS: same widths as Arial, many more characters.
    // "@" in front of a family name means vertical (Asian) text in Windows.
    FallbackRow {
        keys: &["arialuni.ttf", "arial unicode ms", "@arial unicode ms"],
        same: &["arialuni.ttf", "Arial Unicode.ttf"],
        similar: &[
            ("Arial.ttf", 1.0),
            ("arial.ttf", 1.0),
            LIB_SANS,
            ARIMO,
            ("DejaVuSans.ttf", 0.9),
        ],
        builtin: None,
    },
    FallbackRow {
        keys: &["arialbd.ttf", "arial bold"],
        same: &["arialbd.ttf", "Arial Bold.ttf"],
        similar: &[
            ("LiberationSans-Bold.ttf", 1.0),
            ("Arimo-Bold.ttf", 1.0),
            ("DejaVuSans-Bold.ttf", 0.9),
        ],
        builtin: None,
    },
    FallbackRow {
        keys: &["ariali.ttf", "arial italic"],
        same: &["ariali.ttf", "Arial Italic.ttf"],
        similar: &[
            ("LiberationSans-Italic.ttf", 1.0),
            ("Arimo-Italic.ttf", 1.0),
        ],
        builtin: None,
    },
    FallbackRow {
        keys: &["arialbi.ttf", "arial bold italic"],
        same: &["arialbi.ttf", "Arial Bold Italic.ttf"],
        similar: &[
            ("LiberationSans-BoldItalic.ttf", 1.0),
            ("Arimo-BoldItalic.ttf", 1.0),
        ],
        builtin: None,
    },
    FallbackRow {
        keys: &["arialn.ttf", "arial narrow"],
        same: &["arialn.ttf", "Arial Narrow.ttf"],
        similar: &[
            ("LiberationSansNarrow-Regular.ttf", 1.0),
            ("LiberationSans-Regular.ttf", 0.82),
            ("Arimo-Regular.ttf", 0.82),
            ("Arial.ttf", 0.82),
            ("DejaVuSans.ttf", 0.74),
        ],
        builtin: None,
    },
    FallbackRow {
        keys: &["arialnb.ttf", "arial narrow bold"],
        same: &["arialnb.ttf", "Arial Narrow Bold.ttf"],
        similar: &[
            ("LiberationSansNarrow-Bold.ttf", 1.0),
            ("LiberationSans-Bold.ttf", 0.82),
        ],
        builtin: None,
    },
    FallbackRow {
        keys: &["arialni.ttf", "arial narrow italic"],
        same: &["arialni.ttf", "Arial Narrow Italic.ttf"],
        similar: &[
            ("LiberationSansNarrow-Italic.ttf", 1.0),
            ("LiberationSans-Italic.ttf", 0.82),
        ],
        builtin: None,
    },
    // --- Bitstream Swiss 721 (a Helvetica design: Arial metrics are close)
    FallbackRow {
        keys: &[
            "swissl.ttf",
            "swis721 lt bt",
            "swiss.ttf",
            "swis721 bt",
            "swissk.ttf",
        ],
        same: &["swissl.ttf", "swiss.ttf"],
        similar: &[
            LIB_SANS,
            ARIMO,
            ("Arial.ttf", 1.0),
            ("arial.ttf", 1.0),
            ("Helvetica.ttc", 1.0),
            ("DejaVuSans.ttf", 0.9),
        ],
        builtin: None,
    },
    FallbackRow {
        keys: &["swissb.ttf", "swis721 bt bold", "swissbo.ttf"],
        same: &["swissb.ttf"],
        similar: &[
            ("LiberationSans-Bold.ttf", 1.0),
            ("Arimo-Bold.ttf", 1.0),
            ("Arial Bold.ttf", 1.0),
        ],
        builtin: None,
    },
    FallbackRow {
        keys: &[
            "swissc.ttf",
            "swis721 cn bt",
            "swisscl.ttf",
            "swis721 lt cn bt",
        ],
        same: &["swissc.ttf", "swisscl.ttf"],
        similar: &[
            ("LiberationSansNarrow-Regular.ttf", 1.0),
            ("Arial Narrow.ttf", 1.0),
            ("LiberationSans-Regular.ttf", 0.82),
        ],
        builtin: None,
    },
    // --- Century Gothic (TeX Gyre Adventor / URW Gothic share its design)
    FallbackRow {
        keys: &["gothic.ttf", "century gothic"],
        same: &["gothic.ttf", "Century Gothic.ttf"],
        similar: &[
            ("texgyreadventor-regular.otf", 1.0),
            ("URWGothic-Book.otf", 1.0),
            ("URWGothic-Book.ttf", 1.0),
            ("LiberationSans-Regular.ttf", 1.1),
        ],
        builtin: None,
    },
    FallbackRow {
        keys: &["gothicb.ttf", "century gothic bold"],
        same: &["gothicb.ttf", "Century Gothic Bold.ttf"],
        similar: &[
            ("texgyreadventor-bold.otf", 1.0),
            ("URWGothic-Demi.otf", 1.0),
            ("LiberationSans-Bold.ttf", 1.1),
        ],
        builtin: None,
    },
    // --- Technical lettering TrueType fonts: the stroke font is the closest look
    FallbackRow {
        keys: &["stylu.ttf", "stylus bt"],
        same: &["stylu.ttf"],
        similar: &[],
        builtin: Some(BuiltinFont::Simplex),
    },
    FallbackRow {
        keys: &[
            "isocpeur.ttf",
            "isocpeur",
            "isocteur.ttf",
            "isocteur",
            "isocp.ttf",
            "isoct.ttf",
        ],
        same: &["isocpeur.ttf", "isocteur.ttf"],
        similar: &[("osifont.ttf", 1.0)],
        builtin: Some(BuiltinFont::Simplex),
    },
    // --- Others
    FallbackRow {
        keys: &["verdana.ttf", "verdana"],
        same: &["verdana.ttf", "Verdana.ttf"],
        similar: &[("DejaVuSans.ttf", 1.0), ("LiberationSans-Regular.ttf", 1.1)],
        builtin: None,
    },
    FallbackRow {
        keys: &["times.ttf", "times new roman"],
        same: &["times.ttf", "Times New Roman.ttf"],
        similar: &[
            ("LiberationSerif-Regular.ttf", 1.0),
            ("Tinos-Regular.ttf", 1.0),
            ("DejaVuSerif.ttf", 0.9),
        ],
        builtin: None,
    },
    FallbackRow {
        keys: &["cour.ttf", "courier new"],
        same: &["cour.ttf", "Courier New.ttf"],
        similar: &[
            ("LiberationMono-Regular.ttf", 1.0),
            ("Cousine-Regular.ttf", 1.0),
            ("DejaVuSansMono.ttf", 1.0),
        ],
        builtin: None,
    },
    FallbackRow {
        keys: &["calibri.ttf", "calibri"],
        same: &["calibri.ttf"],
        similar: &[("Carlito-Regular.ttf", 1.0), LIB_SANS],
        builtin: None,
    },
    FallbackRow {
        keys: &["symbol.ttf", "symbol"],
        same: &["symbol.ttf", "Symbol.ttf"],
        similar: &[],
        builtin: None,
    },
];

/// Default sans font when a TrueType font has no row or nothing in its row
/// is installed.
pub static DEFAULT_TTF: &[(&str, f32)] = &[
    LIB_SANS,
    ARIMO,
    ("Arial.ttf", 1.0),
    ("arial.ttf", 1.0),
    ("Helvetica.ttc", 1.0),
    ("DejaVuSans.ttf", 0.9),
];

/// Font folders of the operating system, plus the folders in the
/// `CADRENDER_FONT_PATH` environment variable (first, `:` or `;` separated).
pub fn default_font_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Ok(p) = std::env::var("CADRENDER_FONT_PATH") {
        dirs.extend(
            p.split([':', ';'])
                .filter(|s| !s.is_empty())
                .map(PathBuf::from),
        );
    }
    let home = std::env::var_os("HOME").map(PathBuf::from);
    if cfg!(target_os = "macos") {
        dirs.push("/System/Library/Fonts".into());
        dirs.push("/System/Library/Fonts/Supplemental".into());
        dirs.push("/Library/Fonts".into());
        if let Some(h) = &home {
            dirs.push(h.join("Library/Fonts"));
        }
    } else if cfg!(windows) {
        let windir = std::env::var("WINDIR").unwrap_or_else(|_| "C:\\Windows".into());
        dirs.push(PathBuf::from(windir).join("Fonts"));
        if let Ok(l) = std::env::var("LOCALAPPDATA") {
            dirs.push(PathBuf::from(l).join("Microsoft\\Windows\\Fonts"));
        }
    } else {
        dirs.push("/usr/share/fonts".into());
        dirs.push("/usr/local/share/fonts".into());
        if let Some(h) = &home {
            dirs.push(h.join(".local/share/fonts"));
            dirs.push(h.join(".fonts"));
        }
    }
    dirs
}

const MAX_DEPTH: u32 = 6;

fn is_font_file(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    n.ends_with(".shx") || n.ends_with(".ttf") || n.ends_with(".ttc") || n.ends_with(".otf")
}

/// Finds font files by name. Build it once per process (it scans the
/// folders once) and share it.
#[derive(Debug, Clone, Default)]
pub struct FontResolver {
    files: HashMap<String, PathBuf>,
}

impl FontResolver {
    /// A resolver that knows no files (only built-in fonts).
    pub fn empty() -> FontResolver {
        FontResolver::default()
    }

    /// A resolver over [`default_font_dirs`].
    pub fn system() -> FontResolver {
        FontResolver::with_dirs(default_font_dirs())
    }

    /// A resolver over these folders. Earlier folders win when two have a
    /// file with the same name.
    pub fn with_dirs<I, P>(dirs: I) -> FontResolver
    where
        I: IntoIterator<Item = P>,
        P: AsRef<Path>,
    {
        let mut r = FontResolver::empty();
        for d in dirs {
            r.add_dir(d);
        }
        r
    }

    /// Add a folder (searched recursively). Files already known keep their
    /// first path.
    pub fn add_dir(&mut self, dir: impl AsRef<Path>) {
        self.scan(dir.as_ref(), 0);
    }

    /// Add one font file.
    pub fn add_file(&mut self, path: impl AsRef<Path>) {
        let p = path.as_ref();
        if let Some(name) = p.file_name().and_then(|n| n.to_str()) {
            self.files
                .entry(name.to_ascii_lowercase())
                .or_insert_with(|| p.to_path_buf());
        }
    }

    fn scan(&mut self, dir: &Path, depth: u32) {
        let Ok(rd) = std::fs::read_dir(dir) else {
            return;
        };
        let mut entries: Vec<_> = rd.flatten().collect();
        entries.sort_by_key(|e| e.file_name());
        for e in entries {
            let path = e.path();
            let Ok(ft) = e.file_type() else { continue };
            if ft.is_dir() || (ft.is_symlink() && path.is_dir()) {
                if depth < MAX_DEPTH {
                    self.scan(&path, depth + 1);
                }
            } else if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                if is_font_file(name) {
                    self.files.entry(name.to_ascii_lowercase()).or_insert(path);
                }
            }
        }
    }

    /// Number of known font files.
    pub fn len(&self) -> usize {
        self.files.len()
    }

    pub fn is_empty(&self) -> bool {
        self.files.is_empty()
    }

    /// Path of a font file by name (case-insensitive).
    pub fn find_file(&self, file_name: &str) -> Option<&Path> {
        self.files
            .get(&file_name.to_ascii_lowercase())
            .map(|p| p.as_path())
    }

    fn source_for(&self, file_name: &str) -> Option<FontSource> {
        let path = self.find_file(file_name)?.to_path_buf();
        let lower = file_name.to_ascii_lowercase();
        Some(if lower.ends_with(".shx") {
            FontSource::Shx(path)
        } else {
            FontSource::TrueType { path, index: 0 }
        })
    }

    /// Resolve a STYLE font: `font_file` is group code 3, `family` the
    /// TrueType family name from the style's ACAD xdata (if any).
    pub fn resolve(&self, font_file: &str, family: Option<&str>) -> Resolved {
        let file = normalize_file(font_file);
        let fam = family
            .map(|f| f.trim().to_ascii_lowercase())
            .filter(|f| !f.is_empty());
        let requested = if file.is_empty() {
            fam.clone().unwrap_or_default()
        } else {
            file.clone()
        };

        if !file.is_empty() {
            if let Some(source) = self.source_for(&file) {
                return Resolved {
                    source,
                    quality: MatchQuality::Exact,
                    width_scale: 1.0,
                    requested,
                };
            }
        }
        let stem = file.rsplit_once('.').map_or(file.as_str(), |(s, _)| s);
        let row = find_row(&file)
            .or_else(|| find_row(stem))
            .or_else(|| fam.as_deref().and_then(find_row));
        let is_ttf =
            (!file.is_empty() && !file.ends_with(".shx")) || (file.is_empty() && fam.is_some());
        self.pick_from_row(row, is_ttf, requested)
    }

    /// Resolve an MTEXT `\f` family switch (`\fArial|b1|i0;`).
    pub fn resolve_family(&self, family: &str, bold: bool, italic: bool) -> Resolved {
        let base = family.trim().trim_start_matches('@').to_ascii_lowercase();
        let styled = match (bold, italic) {
            (true, true) => format!("{base} bold italic"),
            (true, false) => format!("{base} bold"),
            (false, true) => format!("{base} italic"),
            (false, false) => base.clone(),
        };
        let row = find_row(&styled).or_else(|| find_row(&base));
        self.pick_from_row(row, true, styled)
    }

    /// Resolve an MTEXT `\F` file switch (`\Fromans.shx;` or `\FArial;`).
    pub fn resolve_inline(&self, name: &str) -> Resolved {
        let n = name.trim();
        let lower = n.to_ascii_lowercase();
        if lower.contains('.') || find_row(&lower).is_some_and(|r| r.builtin.is_some()) {
            self.resolve(n, None)
        } else {
            self.resolve("", Some(n))
        }
    }

    /// Resolve a big font (group code 4). Only the exact file is useful:
    /// there are no free substitutes, so `None` means "draw '?' boxes".
    pub fn resolve_bigfont(&self, name: &str) -> Option<Resolved> {
        let file = normalize_file(name);
        if file.is_empty() {
            return None;
        }
        self.source_for(&file).map(|source| Resolved {
            source,
            quality: MatchQuality::Exact,
            width_scale: 1.0,
            requested: file,
        })
    }

    /// Resolve the shape file of a complex linetype (a STYLE with flag bit 1,
    /// for example `ltypeshp.shx`). Shapes are found by number or name, so a
    /// text font is no substitute: `None` means "draw the linetype without
    /// its shapes".
    pub fn resolve_shape_file(&self, name: &str) -> Option<Resolved> {
        self.resolve_bigfont(name)
    }

    fn pick_from_row(
        &self,
        row: Option<&FallbackRow>,
        is_ttf: bool,
        requested: String,
    ) -> Resolved {
        if let Some(row) = row {
            for f in row.same {
                if let Some(source) = self.source_for(f) {
                    return Resolved {
                        source,
                        quality: MatchQuality::SameFont,
                        width_scale: 1.0,
                        requested,
                    };
                }
            }
            for &(f, w) in row.similar {
                if let Some(source) = self.source_for(f) {
                    return Resolved {
                        source,
                        quality: MatchQuality::Similar,
                        width_scale: w,
                        requested,
                    };
                }
            }
            if let Some(b) = row.builtin {
                return Resolved {
                    source: FontSource::Builtin(b),
                    quality: MatchQuality::Similar,
                    width_scale: 1.0,
                    requested,
                };
            }
        }
        if is_ttf {
            for &(f, w) in DEFAULT_TTF {
                if let Some(source) = self.source_for(f) {
                    return Resolved {
                        source,
                        quality: MatchQuality::Fallback,
                        width_scale: w,
                        requested,
                    };
                }
            }
        }
        Resolved {
            source: FontSource::Builtin(BuiltinFont::Simplex),
            quality: MatchQuality::Fallback,
            width_scale: 1.0,
            requested,
        }
    }
}

/// Lower-case file name without folders; `.shx` is added when there is no
/// extension (AutoCAD does the same: "txt" means txt.shx).
pub fn normalize_file(font: &str) -> String {
    let t = font.trim().trim_matches('"');
    let base = t.rsplit(['/', '\\']).next().unwrap_or(t).trim();
    if base.is_empty() {
        return String::new();
    }
    let mut s = base.to_ascii_lowercase();
    if !s.contains('.') {
        s.push_str(".shx");
    }
    s
}

fn find_row(key: &str) -> Option<&'static FallbackRow> {
    if key.is_empty() {
        return None;
    }
    FALLBACKS.iter().find(|r| r.keys.contains(&key))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize() {
        assert_eq!(normalize_file("txt"), "txt.shx");
        assert_eq!(normalize_file(" RomanS.SHX "), "romans.shx");
        assert_eq!(normalize_file("C:\\Fonts\\arial.ttf"), "arial.ttf");
        assert_eq!(normalize_file("/usr/x/isocp"), "isocp.shx");
        assert_eq!(normalize_file(""), "");
    }

    #[test]
    fn empty_resolver_uses_builtins() {
        let r = FontResolver::empty();
        let a = r.resolve("romans.shx", None);
        assert_eq!(a.source, FontSource::Builtin(BuiltinFont::Simplex));
        assert_eq!(a.quality, MatchQuality::Similar);
        let b = r.resolve("romand.shx", None);
        assert_eq!(b.source, FontSource::Builtin(BuiltinFont::Duplex));
        let c = r.resolve("isocp", None);
        assert_eq!(c.source, FontSource::Builtin(BuiltinFont::Simplex));
        let d = r.resolve("unknown_font.shx", None);
        assert_eq!(d.quality, MatchQuality::Fallback);
        let e = r.resolve("arial.ttf", Some("Arial"));
        assert_eq!(e.source, FontSource::Builtin(BuiltinFont::Simplex));
        assert_eq!(e.quality, MatchQuality::Fallback);
        assert!(r.resolve_bigfont("gbcbig.shx").is_none());
    }

    #[test]
    fn resolver_prefers_exact_then_same_then_similar() {
        let dir = std::env::temp_dir().join(format!("cadrender-shx-res-{}", std::process::id()));
        let sub = dir.join("sub");
        std::fs::create_dir_all(&sub).unwrap();
        for f in ["Arial Narrow.ttf", "LiberationSans-Regular.ttf"] {
            std::fs::write(dir.join(f), b"x").unwrap();
        }
        std::fs::write(sub.join("ROMANS.SHX"), b"x").unwrap();
        std::fs::write(dir.join("notes.txt"), b"x").unwrap();
        let r = FontResolver::with_dirs([&dir]);
        assert_eq!(r.len(), 3);

        let a = r.resolve("romans.shx", None);
        assert_eq!(a.quality, MatchQuality::Exact);
        assert!(matches!(a.source, FontSource::Shx(_)));

        let b = r.resolve("arialn.ttf", Some("Arial Narrow"));
        assert_eq!(b.quality, MatchQuality::SameFont);

        let c = r.resolve("swissl.ttf", Some("Swis721 Lt BT"));
        assert_eq!(c.quality, MatchQuality::Similar);
        assert!(
            matches!(&c.source, FontSource::TrueType { path, .. } if path.ends_with("LiberationSans-Regular.ttf"))
        );

        // empty file name, family only
        let d = r.resolve("", Some("Swis721 Lt BT"));
        assert_eq!(d.quality, MatchQuality::Similar);

        // unknown TrueType font: default sans
        let e = r.resolve("weird.ttf", None);
        assert_eq!(e.quality, MatchQuality::Fallback);
        assert!(matches!(e.source, FontSource::TrueType { .. }));

        // MTEXT inline switches
        let f = r.resolve_family("Arial Narrow", false, false);
        assert_eq!(f.quality, MatchQuality::SameFont);
        let g = r.resolve_inline("romans.shx");
        assert_eq!(g.quality, MatchQuality::Exact);
        let h = r.resolve_inline("@Arial Unicode MS");
        assert_eq!(h.quality, MatchQuality::Similar);
        assert_eq!(r.resolve_family("Arial", true, false).width_scale, 1.0);
        std::fs::remove_dir_all(&dir).ok();
    }
}
