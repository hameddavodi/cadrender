//! VIEWPORT.

use super::Common;
use crate::tags::{gf, gi, hex, Tag};
use cadrender_core::entity::viewport::ViewportEnt;
use cadrender_core::entity::Kind;

pub fn viewport(body: &[Tag], com: &Common) -> Option<Kind> {
    let g = |c: i32| gf(body, c).unwrap_or(0.0);
    let frozen = body.iter().filter(|t| t.0 == 331 || t.0 == 341).map(|t| hex(t.1)).collect();
    // a missing view direction means plan view
    let dir = [g(16), g(26), gf(body, 36).unwrap_or(1.0)];
    let top_view = dir[0].abs() < 1e-9 && dir[1].abs() < 1e-9 && dir[2] >= 0.0;
    Some(ViewportEnt {
        handle: com.props.handle,
        center: [g(10), g(20)], w: g(40), h: g(41),
        view_center: [g(12), g(22)], target: [g(17), g(27)], view_h: g(45), twist: g(51),
        status: gi(body, 68).unwrap_or(0), id: gi(body, 69).unwrap_or(0), flags: gi(body, 90).unwrap_or(0),
        frozen_handles: frozen, frozen_layers: vec![], top_view, view_dir: dir, target_z: g(37),
        clip_handle: body.iter().find(|t| t.0 == 340).map(|t| hex(t.1)).unwrap_or(0), clip: None,
    }.into())
}
