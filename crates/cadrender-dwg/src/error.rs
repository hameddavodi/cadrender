//! Error type for the DWG reader.
//!
//! The reader never panics on bad input. Every problem comes back as a
//! [`DwgError`] that says what went wrong and, where it helps, where.

use std::fmt;

/// Everything that can go wrong while reading a DWG file.
#[derive(Debug)]
pub enum DwgError {
    /// The operating system could not open or map the file.
    Io(std::io::Error),
    /// The first bytes are not a DWG version string.
    NotDwg,
    /// A real DWG, but a version this reader does not read (yet).
    UnsupportedVersion(String),
    /// The file uses password encryption for its data sections.
    Encrypted,
    /// A bit or byte read went past the end of its buffer.
    /// `at` and `limit` are bit positions inside that buffer.
    Eof { at: usize, limit: usize },
    /// A checksum (CRC or page checksum) did not match.
    Checksum { what: &'static str, expected: u32, found: u32 },
    /// The data is not what the format allows (bad magic, bad size, ...).
    Corrupt(String),
    /// A section the reader needs is not in the file.
    MissingSection(&'static str),
}

impl DwgError {
    /// Short helper to build a [`DwgError::Corrupt`].
    #[cold]
    pub fn corrupt(msg: impl Into<String>) -> Self {
        DwgError::Corrupt(msg.into())
    }
}

impl fmt::Display for DwgError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DwgError::Io(e) => write!(f, "i/o error: {e}"),
            DwgError::NotDwg => write!(f, "not a DWG file (no AC10xx version string)"),
            DwgError::UnsupportedVersion(v) => write!(f, "DWG version {v} is not supported yet"),
            DwgError::Encrypted => write!(f, "DWG data sections are password encrypted"),
            DwgError::Eof { at, limit } => {
                write!(f, "read past end of data (bit {at}, limit {limit})")
            }
            DwgError::Checksum { what, expected, found } => {
                write!(f, "{what} checksum mismatch: expected {expected:#x}, found {found:#x}")
            }
            DwgError::Corrupt(m) => write!(f, "corrupt DWG data: {m}"),
            DwgError::MissingSection(s) => write!(f, "section {s} is missing"),
        }
    }
}

impl std::error::Error for DwgError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            DwgError::Io(e) => Some(e),
            _ => None,
        }
    }
}

impl From<std::io::Error> for DwgError {
    fn from(e: std::io::Error) -> Self {
        DwgError::Io(e)
    }
}

/// Result type used everywhere in this crate.
pub type Result<T> = std::result::Result<T, DwgError>;
