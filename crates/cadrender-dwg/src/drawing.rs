//! Read a whole DWG file into a [`Drawing`].
//!
//! Steps: memory-map the file, parse the container, decompress the four
//! sections we need (pages in parallel), parse header, classes and object
//! map, then decode all objects in parallel (each object is independent:
//! the object map gives its offset). A broken object does not stop the
//! read; it is listed in [`Drawing::failed`].

use std::collections::BTreeMap;
use std::path::Path;
use std::time::{Duration, Instant};

use rayon::prelude::*;

use crate::classes::{self, DwgClass};
use crate::container::{Container, FileHeader, Version};
use crate::error::{DwgError, Result};
use crate::header::{self, Header};
use crate::model::{Handle, Object};
use crate::objects::{decode_object, ClassTable, DecodeInfo};
use crate::objmap;

/// Options for [`read_bytes`] and [`read_file`].
#[derive(Clone, Debug)]
pub struct ReadOptions {
    /// Check the CRC of every object (cheap; on by default).
    pub check_crc: bool,
    /// Decode objects on all cores with rayon.
    pub parallel: bool,
}

impl Default for ReadOptions {
    fn default() -> Self {
        ReadOptions { check_crc: true, parallel: true }
    }
}

/// Counters per object type, for validation.
#[derive(Clone, Debug, Default)]
pub struct TypeStats {
    pub count: usize,
    /// Type-specific data decoded (not only skipped).
    pub decoded: usize,
    /// Decoding failed with an error.
    pub errors: usize,
    /// Decoded, but the data stream was not used exactly.
    pub data_left: usize,
    /// Decoded, but 8 or more bits stayed in the handle stream.
    pub handles_left: usize,
    pub crc_bad: usize,
}

/// Time spent per step.
#[derive(Clone, Debug, Default)]
pub struct Timings {
    pub container: Duration,
    pub sections: Duration,
    pub header_classes_map: Duration,
    pub objects: Duration,
    /// Part of `objects`: the (parallel) decode itself.
    pub objects_decode: Duration,
    pub total: Duration,
}

/// A decoded DWG file.
#[derive(Debug)]
pub struct Drawing {
    pub version: Version,
    pub file_header: FileHeader,
    pub header: Header,
    pub classes: Vec<DwgClass>,
    /// All decoded objects, sorted by handle.
    pub objects: Vec<Object>,
    /// Objects that could not be decoded: (handle, type name, error).
    pub failed: Vec<(Handle, String, String)>,
    /// Statistics per type name.
    pub stats: BTreeMap<String, TypeStats>,
    /// Problems that did not stop reading (checksums, ...).
    pub warnings: Vec<String>,
    pub timings: Timings,
}

impl Drawing {
    /// Find an object by handle.
    pub fn get(&self, handle: Handle) -> Option<&Object> {
        self.objects
            .binary_search_by_key(&handle, |o| o.handle)
            .ok()
            .map(|i| &self.objects[i])
    }

    /// Type name of an object (fixed name or class DXF name).
    pub fn type_name(&self, type_code: u16) -> &str {
        ClassTable::new(&self.classes).name(type_code)
    }
}

/// Entities with a colour-book colour store only a DBCOLOR handle. Copy the
/// DBCOLOR's colour into the entity, so users see the real colour.
/// One section read: its bytes (if present) and the warnings found.
type SectionRead = (Result<Option<Vec<u8>>>, Vec<String>);

fn resolve_book_colors(objects: &mut [Object]) {
    use crate::model::ObjectKind;
    let books: std::collections::HashMap<Handle, crate::streams::Color> = objects
        .iter()
        .filter_map(|o| match &o.kind {
            ObjectKind::DbColor { color, .. } => Some((o.handle, *color)),
            _ => None,
        })
        .collect();
    if books.is_empty() {
        return;
    }
    for o in objects.iter_mut() {
        if let ObjectKind::Entity(e) = &mut o.kind {
            if let Some(b) = e.common.color_book.and_then(|h| books.get(&h)) {
                e.common.color.color = *b;
            }
        }
    }
}

/// Entities of *Model_Space and *Paper_Space do not store their owner
/// (entity mode 2 / 1). Fill it in from the header, so every entity has one.
fn fill_space_owners(objects: &mut [Object], model_space: Handle, paper_space: Handle) {
    use crate::model::ObjectKind;
    for o in objects.iter_mut() {
        if let ObjectKind::Entity(e) = &o.kind {
            if o.owner == 0 {
                match e.common.entmode {
                    1 => o.owner = paper_space,
                    2 => o.owner = model_space,
                    _ => {}
                }
            }
        }
    }
}

/// Memory-map and read a DWG file.
pub fn read_file(path: impl AsRef<Path>, opts: &ReadOptions) -> Result<Drawing> {
    let f = std::fs::File::open(path.as_ref())?;
    // SAFETY: the map is read-only and lives only during this call. If
    // another process truncates the file meanwhile, reads may fault; this is
    // the usual memmap trade-off for speed and is documented in the README.
    let map = unsafe { memmap2::Mmap::map(&f)? };
    read_bytes(&map, opts)
}

/// Read a DWG file that is already in memory.
pub fn read_bytes(data: &[u8], opts: &ReadOptions) -> Result<Drawing> {
    let t0 = Instant::now();
    let mut timings = Timings::default();
    let c = Container::parse(data)?;
    timings.container = t0.elapsed();
    let ver = c.header.version;
    let maint = c.header.maint_release;
    let mut warnings = c.warnings.clone();

    let t = Instant::now();
    let names = ["AcDb:Header", "AcDb:Classes", "AcDb:Handles", "AcDb:AcDbObjects"];
    let results: Vec<SectionRead> = names
        .par_iter()
        .map(|n| {
            let mut w = Vec::new();
            (c.section(n, &mut w), w)
        })
        .collect();
    let mut bufs = Vec::with_capacity(4);
    for ((r, w), name) in results.into_iter().zip(names) {
        warnings.extend(w);
        let b = r?.ok_or(DwgError::MissingSection(name))?;
        bufs.push(b);
    }
    let objects_buf = bufs.pop().unwrap_or_default();
    let handles_buf = bufs.pop().unwrap_or_default();
    let classes_buf = bufs.pop().unwrap_or_default();
    let header_buf = bufs.pop().unwrap_or_default();
    timings.sections = t.elapsed();

    let t = Instant::now();
    let header = header::parse(&header_buf, ver, maint)?;
    let classes = classes::parse(&classes_buf, ver, maint)?;
    let (map, bad_crc) = objmap::parse(&handles_buf)?;
    if bad_crc > 0 {
        warnings.push(format!("object map: {bad_crc} blocks with a bad CRC"));
    }
    timings.header_classes_map = t.elapsed();

    let t = Instant::now();
    let table = ClassTable::new(&classes);
    let decode = |e: &objmap::MapEntry| -> (Handle, Result<(Object, DecodeInfo)>) {
        let r = usize::try_from(e.offset)
            .map_err(|_| DwgError::corrupt("offset too large"))
            .and_then(|off| decode_object(&objects_buf, off, ver, &table, opts.check_crc));
        (e.handle, r)
    };
    let results: Vec<_> = if opts.parallel {
        map.par_iter().with_min_len(256).map(decode).collect()
    } else {
        map.iter().map(decode).collect()
    };
    timings.objects_decode = t.elapsed();

    let mut objects = Vec::with_capacity(results.len());
    let mut failed = Vec::new();
    // Count by type number first (no string work per object), name at the end.
    let mut by_code: std::collections::HashMap<u16, TypeStats> = std::collections::HashMap::new();
    let mut stats: BTreeMap<String, TypeStats> = BTreeMap::new();
    let mut handle_mismatch = 0usize;
    for (map_handle, r) in results {
        match r {
            Ok((obj, info)) => {
                let s = by_code.entry(obj.type_code).or_default();
                s.count += 1;
                if info.decoded {
                    s.decoded += 1;
                    if info.data_left != 0 {
                        s.data_left += 1;
                    }
                    if info.handles_left >= 8 {
                        s.handles_left += 1;
                    }
                }
                if !info.crc_ok {
                    s.crc_bad += 1;
                }
                if obj.handle != map_handle {
                    handle_mismatch += 1;
                }
                objects.push(obj);
            }
            Err(e) => {
                // Find the type for the statistics, if the frame is readable.
                let name = map
                    .binary_search_by_key(&map_handle, |m| m.handle)
                    .ok()
                    .and_then(|i| crate::objects::peek_type(&objects_buf, map[i].offset as usize, ver).ok())
                    .map(|(code, _)| table.name(code).to_string())
                    .unwrap_or_else(|| "UNREADABLE".to_string());
                let s = stats.entry(name.clone()).or_default();
                s.count += 1;
                s.errors += 1;
                let off = map
                    .binary_search_by_key(&map_handle, |m| m.handle)
                    .map(|i| map[i].offset)
                    .unwrap_or(0);
                failed.push((map_handle, name, format!("{e} (offset {off:#x})")));
            }
        }
    }
    for (code, s) in by_code {
        let e = stats.entry(table.name(code).to_string()).or_default();
        e.count += s.count;
        e.decoded += s.decoded;
        e.errors += s.errors;
        e.data_left += s.data_left;
        e.handles_left += s.handles_left;
        e.crc_bad += s.crc_bad;
    }
    if handle_mismatch > 0 {
        warnings.push(format!("{handle_mismatch} objects have a handle different from the object map"));
    }
    objects.sort_by_key(|o| o.handle);
    crate::acis::fill_from_acds(&c, &mut objects, &mut warnings);
    resolve_book_colors(&mut objects);
    fill_space_owners(&mut objects, header.handles.model_space, header.handles.paper_space);
    timings.objects = t.elapsed();
    timings.total = t0.elapsed();

    Ok(Drawing {
        version: ver,
        file_header: c.header.clone(),
        header,
        classes,
        objects,
        failed,
        stats,
        warnings,
        timings,
    })
}
