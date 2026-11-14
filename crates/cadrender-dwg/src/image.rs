//! IMAGEDEF (ODA spec): class version, image size, file path, loaded flag,
//! resolution units, pixel size. Only the file path is kept.

use crate::error::Result;
use crate::model::ObjectKind;
use crate::streams::ObjReader;

pub(crate) fn imagedef(r: &mut ObjReader<'_>) -> Result<ObjectKind> {
    r.d.bl()?; // class version
    r.d.rd2()?; // size in pixels
    let file = r.tv()?;
    // read the rest too, so the bit self-check sees the whole object
    r.d.b()?; // is loaded
    r.d.rc()?; // resolution units
    r.d.rd2()?; // pixel size
    Ok(ObjectKind::ImageDef { file })
}
