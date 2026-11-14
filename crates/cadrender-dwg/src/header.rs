//! The AcDb:Header section: drawing variables (ODA spec chapter 9).
//!
//! The variables are one long bit stream with no names, so the reader must
//! walk every field in order, even the ones we do not keep. This module
//! follows the spec's field list for R2004 and later (R2007+ strings and
//! handles come from their own streams, see [`crate::streams`]).

use crate::bits::BitReader;
use crate::container::Version;
use crate::error::{DwgError, Result};
use crate::streams::{string_stream, Color, ObjReader};

const START_SENTINEL: [u8; 16] =
    [0xCF, 0x7B, 0x1F, 0x23, 0xFD, 0xDE, 0x38, 0xA9, 0x5F, 0x7C, 0x68, 0xB8, 0x4E, 0x6D, 0x33, 0x5F];

/// Handles to the tables and dictionaries, from the header.
#[derive(Clone, Debug, Default)]
pub struct HeaderHandles {
    pub clayer: u64,
    pub textstyle: u64,
    pub celtype: u64,
    pub cmaterial: u64,
    pub dimstyle: u64,
    pub cmlstyle: u64,
    pub block_control: u64,
    pub layer_control: u64,
    pub style_control: u64,
    pub ltype_control: u64,
    pub view_control: u64,
    pub ucs_control: u64,
    pub vport_control: u64,
    pub appid_control: u64,
    pub dimstyle_control: u64,
    pub dict_acad_group: u64,
    pub dict_acad_mlinestyle: u64,
    pub dict_named_objects: u64,
    pub dict_layouts: u64,
    pub dict_plotsettings: u64,
    pub dict_plotstyles: u64,
    pub dict_materials: u64,
    pub dict_colors: u64,
    pub dict_visualstyle: u64,
    pub paper_space: u64,
    pub model_space: u64,
    pub ltype_bylayer: u64,
    pub ltype_byblock: u64,
    pub ltype_continuous: u64,
}

/// The header variables the renderer needs (and a few for checks).
#[derive(Clone, Debug, Default)]
pub struct Header {
    pub psltscale: bool,
    pub tilemode: bool,
    pub lunits: i16,
    pub luprec: i16,
    pub aunits: i16,
    pub pdmode: i16,
    pub ltscale: f64,
    pub textsize: f64,
    pub pdsize: f64,
    pub celtscale: f64,
    pub cecolor: Color,
    pub handseed: u64,
    pub psvpscale: f64,
    pub p_insbase: [f64; 3],
    pub p_extmin: [f64; 3],
    pub p_extmax: [f64; 3],
    pub p_limmin: [f64; 2],
    pub p_limmax: [f64; 2],
    pub insbase: [f64; 3],
    pub extmin: [f64; 3],
    pub extmax: [f64; 3],
    pub limmin: [f64; 2],
    pub limmax: [f64; 2],
    pub dimscale: f64,
    pub dimtxt: f64,
    /// CELWEIGHT, ENDCAPS, JOINSTYLE, LWDISPLAY, ... packed (spec 9).
    pub flags: i32,
    pub insunits: i16,
    pub fingerprint_guid: String,
    pub version_guid: String,
    pub sortents: u8,
    pub handles: HeaderHandles,
}

impl Header {
    /// LWDISPLAY: are lineweights shown?
    pub fn lwdisplay(&self) -> bool {
        self.flags & 0x0200 == 0
    }
}

/// A header handle: from the handle stream (R2007+) or, before R2007,
/// from the data stream where it sits between the values.
fn href(d: &mut ObjReader<'_>) -> Result<crate::bits::HandleRef> {
    if d.ver.r2007_plus() { d.href() } else { d.d.h() }
}

/// [`href`] as an absolute handle (header handles are never relative).
fn href_abs(d: &mut ObjReader<'_>) -> Result<u64> {
    Ok(href(d)?.absolute(0))
}

/// Parse the decompressed AcDb:Header section.
pub fn parse(buf: &[u8], ver: Version, maint_release: u8) -> Result<Header> {
    if buf.len() < 24 || buf[..16] != START_SENTINEL {
        return Err(DwgError::corrupt("header section start sentinel not found"));
    }
    let mut d = BitReader::new(buf);
    d.skip(16 * 8)?;
    let size = d.rl()? as usize;
    if (ver.r2010_plus() && maint_release > 3) || ver.r2018_plus() {
        d.rl()?;
    }
    let initial = d.pos();
    let data_end = initial + size * 8;
    if data_end > buf.len() * 8 {
        return Err(DwgError::corrupt("header size larger than its section"));
    }
    d.set_end(data_end);

    let mut r = if ver.r2007_plus() {
        let size_bits = d.rl()? as usize;
        let flag_pos = initial + size_bits - 1;
        if size_bits == 0 || flag_pos >= data_end {
            return Err(DwgError::corrupt("header bit size out of range"));
        }
        let s = string_stream(buf, initial, flag_pos)?;
        let h = BitReader::with_range(buf, flag_pos + 1, data_end);
        let mut d2 = d;
        d2.set_end(flag_pos);
        ObjReader { d: d2, s, h, ver, handle: 0 }
    } else {
        ObjReader { d, s: None, h: d, ver, handle: 0 }
    };
    // Before R2007 the header has no separate handle stream: handles sit in
    // the data stream between the values (see `href` and `href_raw`).

    let mut hd = Header::default();
    let d = &mut r;

    if ver.r2013_plus() {
        d.d.bll()?; // REQUIREDVERSIONS
    }
    for _ in 0..4 {
        d.d.bd()?;
    }
    for _ in 0..4 {
        d.tv()?;
    }
    d.d.bl()?;
    d.d.bl()?;
    // DIMASO DIMSHO PLINEGEN ORTHOMODE REGENMODE FILLMODE QTEXTMODE
    for _ in 0..7 {
        d.d.b()?;
    }
    hd.psltscale = d.d.b()?;
    d.d.b()?; // LIMCHECK
    d.d.b()?; // undocumented (R2004+)
    // USRTIMER SKPOLY ANGDIR SPLFRAME MIRRTEXT WORLDVIEW
    for _ in 0..6 {
        d.d.b()?;
    }
    hd.tilemode = d.d.b()?;
    // PLIMCHECK VISRETAIN DISPSILH PELLIPSE
    for _ in 0..4 {
        d.d.b()?;
    }
    d.d.bs()?; // PROXYGRAPHICS
    d.d.bs()?; // TREEDEPTH
    hd.lunits = d.d.bs()?;
    hd.luprec = d.d.bs()?;
    hd.aunits = d.d.bs()?;
    d.d.bs()?; // AUPREC
    d.d.bs()?; // ATTMODE
    hd.pdmode = d.d.bs()?;
    for _ in 0..3 {
        d.d.bl()?; // unknown R2004+
    }
    // USERI1-5, SPLINESEGS, SURFU, SURFV, SURFTYPE, SURFTAB1, SURFTAB2,
    // SPLINETYPE, SHADEDGE, SHADEDIF, UNITMODE, MAXACTVP, ISOLINES,
    // CMLJUST, TEXTQLTY
    for _ in 0..19 {
        d.d.bs()?;
    }
    hd.ltscale = d.d.bd()?;
    hd.textsize = d.d.bd()?;
    d.d.bd()?; // TRACEWID
    d.d.bd()?; // SKETCHINC
    d.d.bd()?; // FILLETRAD
    d.d.bd()?; // THICKNESS
    d.d.bd()?; // ANGBASE
    hd.pdsize = d.d.bd()?;
    // PLINEWID USERR1-5 CHAMFERA-D FACETRES CMLSCALE
    for _ in 0..12 {
        d.d.bd()?;
    }
    hd.celtscale = d.d.bd()?;
    d.tv()?; // MENUNAME (still written by R2018 files, see README)
    // TDCREATE, TDUPDATE (2 BL each), 3 unknown BL, TDINDWG, TDUSRTIMER
    for _ in 0..11 {
        d.d.bl()?;
    }
    hd.cecolor = d.cmc()?;
    hd.handseed = d.d.h()?.value; // HANDSEED is in the data stream
    let hh = &mut hd.handles;
    hh.clayer = href_abs(d)?;
    hh.textstyle = href_abs(d)?;
    hh.celtype = href_abs(d)?;
    if ver.r2007_plus() {
        hh.cmaterial = href_abs(d)?;
    }
    hh.dimstyle = href_abs(d)?;
    hh.cmlstyle = href_abs(d)?;
    hd.psvpscale = d.d.bd()?;
    hd.p_insbase = d.d.bd3()?;
    hd.p_extmin = d.d.bd3()?;
    hd.p_extmax = d.d.bd3()?;
    hd.p_limmin = d.d.rd2()?;
    hd.p_limmax = d.d.rd2()?;
    d.d.bd()?; // PELEVATION
    for _ in 0..3 {
        d.d.bd3()?; // PUCSORG, PUCSXDIR, PUCSYDIR
    }
    href(d)?; // PUCSNAME
    href(d)?; // PUCSORTHOREF
    d.d.bs()?; // PUCSORTHOVIEW
    href(d)?; // PUCSBASE
    for _ in 0..6 {
        d.d.bd3()?; // PUCSORG TOP/BOTTOM/LEFT/RIGHT/FRONT/BACK
    }
    hd.insbase = d.d.bd3()?;
    hd.extmin = d.d.bd3()?;
    hd.extmax = d.d.bd3()?;
    hd.limmin = d.d.rd2()?;
    hd.limmax = d.d.rd2()?;
    d.d.bd()?; // ELEVATION
    for _ in 0..3 {
        d.d.bd3()?;
    }
    href(d)?; // UCSNAME
    href(d)?; // UCSORTHOREF
    d.d.bs()?; // UCSORTHOVIEW
    href(d)?; // UCSBASE
    for _ in 0..6 {
        d.d.bd3()?;
    }
    d.tv()?; // DIMPOST
    d.tv()?; // DIMAPOST
    hd.dimscale = d.d.bd()?;
    // DIMASZ DIMEXO DIMDLI DIMEXE DIMRND DIMDLE DIMTP DIMTM
    for _ in 0..8 {
        d.d.bd()?;
    }
    if ver.r2007_plus() {
        // DIMFXL DIMJOGANG DIMTFILL DIMTFILLCLR
        d.d.bd()?;
        d.d.bd()?;
        d.d.bs()?;
        d.cmc()?;
    }
    // DIMTOL DIMLIM DIMTIH DIMTOH DIMSE1 DIMSE2
    for _ in 0..6 {
        d.d.b()?;
    }
    d.d.bs()?; // DIMTAD
    d.d.bs()?; // DIMZIN
    d.d.bs()?; // DIMAZIN
    if ver.r2007_plus() {
        d.d.bs()?; // DIMARCSYM
    }
    hd.dimtxt = d.d.bd()?;
    // DIMCEN DIMTSZ DIMALTF DIMLFAC DIMTVP DIMTFAC DIMGAP DIMALTRND
    for _ in 0..8 {
        d.d.bd()?;
    }
    d.d.b()?; // DIMALT
    d.d.bs()?; // DIMALTD
    for _ in 0..4 {
        d.d.b()?; // DIMTOFL DIMSAH DIMTIX DIMSOXD
    }
    for _ in 0..3 {
        d.cmc()?; // DIMCLRD DIMCLRE DIMCLRT
    }
    // DIMADEC DIMDEC DIMTDEC DIMALTU DIMALTTD DIMAUNIT DIMFRAC DIMLUNIT
    // DIMDSEP DIMTMOVE DIMJUST
    for _ in 0..11 {
        d.d.bs()?;
    }
    d.d.b()?; // DIMSD1
    d.d.b()?; // DIMSD2
    for _ in 0..4 {
        d.d.bs()?; // DIMTOLJ DIMTZIN DIMALTZ DIMALTTZ
    }
    d.d.b()?; // DIMUPT
    d.d.bs()?; // DIMATFIT
    if ver.r2007_plus() {
        d.d.b()?; // DIMFXLON
    }
    if ver.r2010_plus() {
        d.d.b()?; // DIMTXTDIRECTION
        d.d.bd()?; // DIMALTMZF
        d.tv()?; // DIMALTMZS
        d.d.bd()?; // DIMMZF
        d.tv()?; // DIMMZS
    }
    for _ in 0..5 {
        href(d)?; // DIMTXSTY DIMLDRBLK DIMBLK DIMBLK1 DIMBLK2
    }
    if ver.r2007_plus() {
        for _ in 0..3 {
            href(d)?; // DIMLTYPE DIMLTEX1 DIMLTEX2
        }
    }
    d.d.bs()?; // DIMLWD
    d.d.bs()?; // DIMLWE
    hh.block_control = href_abs(d)?;
    hh.layer_control = href_abs(d)?;
    hh.style_control = href_abs(d)?;
    hh.ltype_control = href_abs(d)?;
    hh.view_control = href_abs(d)?;
    hh.ucs_control = href_abs(d)?;
    hh.vport_control = href_abs(d)?;
    hh.appid_control = href_abs(d)?;
    hh.dimstyle_control = href_abs(d)?;
    hh.dict_acad_group = href_abs(d)?;
    hh.dict_acad_mlinestyle = href_abs(d)?;
    hh.dict_named_objects = href_abs(d)?;
    d.d.bs()?; // TSTACKALIGN
    d.d.bs()?; // TSTACKSIZE
    d.tv()?; // HYPERLINKBASE
    d.tv()?; // STYLESHEET
    hh.dict_layouts = href_abs(d)?;
    hh.dict_plotsettings = href_abs(d)?;
    hh.dict_plotstyles = href_abs(d)?;
    hh.dict_materials = href_abs(d)?;
    hh.dict_colors = href_abs(d)?;
    if ver.r2007_plus() {
        hh.dict_visualstyle = href_abs(d)?;
    }
    if ver.r2013_plus() {
        href(d)?; // unknown
    }
    hd.flags = d.d.bl()?;
    hd.insunits = d.d.bs()?;
    let cepsntype = d.d.bs()?;
    if cepsntype == 3 {
        href(d)?; // CPSNID
    }
    hd.fingerprint_guid = d.tv()?;
    hd.version_guid = d.tv()?;
    hd.sortents = d.d.rc()?;
    // INDEXCTL HIDETEXT XCLIPFRAME DIMASSOC HALOGAP
    for _ in 0..5 {
        d.d.rc()?;
    }
    d.d.bs()?; // OBSCUREDCOLOR
    d.d.bs()?; // INTERSECTIONCOLOR
    d.d.rc()?; // OBSCUREDLTYPE
    d.d.rc()?; // INTERSECTIONDISPLAY
    d.tv()?; // PROJECTNAME
    let hh = &mut hd.handles;
    hh.paper_space = href_abs(d)?;
    hh.model_space = href_abs(d)?;
    hh.ltype_bylayer = href_abs(d)?;
    hh.ltype_byblock = href_abs(d)?;
    hh.ltype_continuous = href_abs(d)?;
    // The rest (camera, lofting, shadow settings) is not needed.
    Ok(hd)
}
