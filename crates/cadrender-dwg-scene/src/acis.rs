//! REGION, 3DSOLID, BODY: their outline from the ACIS data, same as the DXF
//! reader (`cadrender-dxf/src/entities/acis.rs`). The DWG reader already
//! deciphered SAT text; SAB is read as it is. R2013+ files keep the data in
//! the AcDs section, which is not read yet: those are counted and skipped.

use crate::tables::Ctx;
use cadrender_core::entity::stroke::StrokeEnt;
use cadrender_core::entity::Kind;
use cadrender_dwg::acis::Acis;

impl Ctx<'_> {
    pub fn acis(&mut self, a: &Acis, name: &str) -> Option<Kind> {
        if a.data.is_empty() { self.b.skip(&format!("{name}(ACIS in AcDs section)")); return None; }
        let Some(o) = cadrender_acis::outline(&a.data) else { self.b.skip("ACIS(unreadable)"); return None };
        if o.skipped > 0 {
            for _ in 0..o.skipped { self.b.skip("ACIS_SPLINE_EDGE"); }
            self.b.warn("REGION/3DSOLID/BODY: spline edges are not drawn");
        }
        if o.path.is_empty() { return None; }
        Some(StrokeEnt { path: o.path, width: 0.0 }.into())
    }
}
