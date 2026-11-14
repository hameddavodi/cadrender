//! A tiny bit writer to build DWG object fixtures by hand (R2010+ layout).
#![allow(dead_code)]

use cadrender_dwg::crc::crc16;

#[derive(Default, Clone)]
pub struct BitWriter {
    pub bits: Vec<bool>,
}

impl BitWriter {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn len(&self) -> usize {
        self.bits.len()
    }
    pub fn bit(&mut self, b: bool) -> &mut Self {
        self.bits.push(b);
        self
    }
    pub fn n(&mut self, v: u64, n: u32) -> &mut Self {
        for i in (0..n).rev() {
            self.bits.push((v >> i) & 1 == 1);
        }
        self
    }
    pub fn rc(&mut self, v: u8) -> &mut Self {
        self.n(v as u64, 8)
    }
    pub fn rs(&mut self, v: u16) -> &mut Self {
        for b in v.to_le_bytes() {
            self.rc(b);
        }
        self
    }
    pub fn rl(&mut self, v: u32) -> &mut Self {
        for b in v.to_le_bytes() {
            self.rc(b);
        }
        self
    }
    pub fn rd(&mut self, v: f64) -> &mut Self {
        for b in v.to_le_bytes() {
            self.rc(b);
        }
        self
    }
    pub fn bb(&mut self, v: u8) -> &mut Self {
        self.n(v as u64, 2)
    }
    pub fn bs(&mut self, v: i16) -> &mut Self {
        match v {
            0 => self.bb(2),
            256 => self.bb(3),
            1..=255 => self.bb(1).rc(v as u8),
            _ => self.bb(0).rs(v as u16),
        }
    }
    pub fn bl(&mut self, v: i32) -> &mut Self {
        match v {
            0 => self.bb(2),
            1..=255 => self.bb(1).rc(v as u8),
            _ => self.bb(0).rl(v as u32),
        }
    }
    pub fn bd(&mut self, v: f64) -> &mut Self {
        if v == 1.0 {
            self.bb(1)
        } else if v == 0.0 {
            self.bb(2)
        } else {
            self.bb(0).rd(v)
        }
    }
    /// DD equal to its default.
    pub fn dd_default(&mut self) -> &mut Self {
        self.bb(0)
    }
    /// Handle reference CODE.COUNTER.BYTES (MSB first).
    pub fn h(&mut self, code: u8, v: u64) -> &mut Self {
        let nbytes = (64 - v.leading_zeros()).div_ceil(8);
        self.rc((code << 4) | nbytes as u8);
        for i in (0..nbytes).rev() {
            self.rc((v >> (8 * i)) as u8);
        }
        self
    }
    /// T text (R2004: BS length + 8-bit bytes).
    pub fn t8(&mut self, s: &str) -> &mut Self {
        self.bs(s.len() as i16);
        for c in s.bytes() {
            self.rc(c);
        }
        self
    }
    /// TU text (BS length + UTF-16LE).
    pub fn tu(&mut self, s: &str) -> &mut Self {
        let u: Vec<u16> = s.encode_utf16().collect();
        self.bs(u.len() as i16);
        for c in u {
            self.rs(c);
        }
        self
    }
    pub fn append(&mut self, o: &BitWriter) -> &mut Self {
        self.bits.extend_from_slice(&o.bits);
        self
    }
    pub fn bytes(&self) -> Vec<u8> {
        let mut out = vec![0u8; self.bits.len().div_ceil(8)];
        for (i, b) in self.bits.iter().enumerate() {
            if *b {
                out[i / 8] |= 0x80 >> (i % 8);
            }
        }
        out
    }
}

/// Build one complete R2010+ object: MS size, MC handle-stream bits, data,
/// string stream (when `strings` is not empty), flag bit, handles, CRC.
/// Returns the bytes of a fake objects section with the object at offset 0.
pub fn object(data: &BitWriter, strings: &BitWriter, handles: &BitWriter) -> Vec<u8> {
    let mut body = data.clone();
    if strings.len() > 0 {
        body.append(strings);
        body.rs(strings.len() as u16);
        body.bit(true);
    } else {
        body.bit(false);
    }
    let mut h = handles.clone();
    while (body.len() + h.len()) % 8 != 0 {
        h.bit(false);
    }
    let hsize = h.len();
    body.append(&h);
    let obj = body.bytes();
    let mut out = Vec::new();
    // MS (size < 0x8000) and MC (handle bits < 0x80 keep it one byte)
    out.extend_from_slice(&(obj.len() as u16).to_le_bytes());
    assert!(hsize < 0x80, "fixture handle stream too long for 1-byte MC");
    out.push(hsize as u8);
    out.extend_from_slice(&obj);
    let crc = crc16(0xC0C1, &out);
    out.extend_from_slice(&crc.to_le_bytes());
    out.extend_from_slice(&[0u8; 8]); // room after the object
    out
}

/// Build one complete R2004 object: MS size, then the data (BS type, RL
/// size of the data in bits, handle, no EED, `rest`), then the handles, CRC.
/// No string stream: texts are 8-bit in the data.
pub fn object_r2004(type_code: u16, handle: u64, rest: &BitWriter, handles: &BitWriter) -> Vec<u8> {
    let mut head = BitWriter::new();
    head.h(0, handle).bs(0); // handle, no EED
    let data_bits = |rl: u32| { let mut t = BitWriter::new(); t.bs(type_code as i16).rl(rl); t.len() + head.len() + rest.len() };
    let bits = data_bits(0);
    let mut body = BitWriter::new();
    body.bs(type_code as i16).rl(bits as u32).append(&head).append(rest);
    assert_eq!(body.len(), bits);
    body.append(handles);
    while body.len() % 8 != 0 {
        body.bit(false);
    }
    let obj = body.bytes();
    let mut out = Vec::new();
    out.extend_from_slice(&(obj.len() as u16).to_le_bytes());
    out.extend_from_slice(&obj);
    let crc = crc16(0xC0C1, &out);
    out.extend_from_slice(&crc.to_le_bytes());
    out.extend_from_slice(&[0u8; 8]);
    out
}

/// Common entity data with "simple" values: model space, no reactors, no
/// xdictionary, colour by layer, ltype by layer, lineweight by layer.
pub fn entity_common(d: &mut BitWriter, type_code: u16, handle: u64) {
    d.bb(0).rc(type_code as u8); // OT (R2010 form, one byte)
    d.h(0, handle);
    d.bs(0); // no EED
    d.bit(false); // no graphics
    d.bb(2); // entmode: model space
    d.bl(0); // reactors
    d.bit(true); // xdic missing
    d.bit(false); // no DS data (R2013+)
    d.bs(256); // ENC: by layer
    d.bd(1.0); // ltype scale
    d.bb(0); // ltype by layer
    d.bb(0); // material by layer
    d.rc(0); // shadow flags
    d.bb(0); // plotstyle by layer
    d.bit(false).bit(false).bit(false); // visual styles
    d.bs(0); // visible
    d.rc(29); // lineweight by layer
}
