//! cadrender-acis: read just enough of ACIS data to draw the outline of a
//! REGION, 3DSOLID or BODY, seen from above.
//!
//! AutoCAD stores these entities as ACIS model data:
//! - SAT text (DXF R2000-R2010 groups 1/3, DWG up to R2010), with a simple
//!   character cipher ([`decipher`]);
//! - SAB binary (DXF R2013+ in the ACDSDATA section, DWG R2013+).
//!
//! Straight and circular/elliptic edges are drawn. Spline edges are counted
//! in [`Outline::skipped`] and left out. Written from the public format
//! descriptions (ezdxf's MIT-licensed notes on the SAB tags); no GPL code.

mod edges;
pub mod tokens;

pub use edges::Outline;
pub use tokens::decipher;

/// Outline of ACIS data: SAB if it starts with a SAB signature, else SAT
/// text (already deciphered). `None` if the data cannot be read at all.
pub fn outline(data: &[u8]) -> Option<Outline> {
    let file = if data.starts_with(b"ACIS BinaryFile") || data.starts_with(b"ASM BinaryFile") {
        tokens::parse_sab(data)?
    } else {
        tokens::parse_sat(&String::from_utf8_lossy(data))?
    };
    Some(edges::outline(&file))
}

#[cfg(test)]
mod tests;
