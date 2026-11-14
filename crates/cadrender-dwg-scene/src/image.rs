//! IMAGE: same as the DXF reader (`cadrender-dxf/src/entities/image.rs`):
//! the picture matrix, the clip frame, brightness/contrast/fade and the
//! IMAGEDEF file name. The picture is loaded later by cadrender-image.

use crate::tables::Ctx;
use cadrender_core::entity::image::ImageEnt;
use cadrender_core::entity::Kind;
use cadrender_core::geom::PathData;
use cadrender_dwg::{ObjectKind, Wipeout};

impl Ctx<'_> {
    pub fn image(&self, w: &Wipeout) -> Option<Kind> {
        let (pw, ph) = (w.size[0], w.size[1]);
        if !(pw > 0.0 && ph > 0.0 && pw.is_finite() && ph.is_finite()) { return None; }
        let m = ImageEnt::matrix([w.insertion[0], w.insertion[1]], [w.u_vector[0], w.u_vector[1]], [w.v_vector[0], w.v_vector[1]], ph);
        let frame = match (w.clipping, crate::kinds::wipeout(w)) {
            (true, Some(Kind::Fill(f))) => f.path,
            _ => {
                let mut p = PathData::default();
                for (k, (x, y)) in [(0.0, 0.0), (pw, 0.0), (pw, ph), (0.0, ph)].into_iter().enumerate() {
                    let (x, y) = m.apply(x, y);
                    if k == 0 { p.move_to(x, y) } else { p.line_to(x, y) }
                }
                p.close();
                p
            }
        };
        let file = match self.d.get(w.imagedef).map(|o| &o.kind) { Some(ObjectKind::ImageDef { file }) => file.clone(), _ => String::new() };
        Some(ImageEnt {
            m, size: [pw, ph], frame,
            show: w.display_flags & 1 != 0,
            brightness: w.brightness.min(100), contrast: w.contrast.min(100), fade: w.fade.min(100),
            file, picture: None,
        }.into())
    }
}
