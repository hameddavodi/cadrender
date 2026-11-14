//! Raster pictures (IMAGE entities): each device pixel inside the clip
//! polygon (anti-aliased edge, from the polygon filler) takes the average
//! of a few picture samples, with brightness/contrast (a table), grey for
//! the monochrome look, and fade toward the page colour.

use crate::canvas::{div255, Canvas, Format, GrayAlphaPx, GrayPx, Px, RgbaPx};
use crate::fill::Filler;
use cadrender_core::sink::{FillRule, ImageDraw};

pub fn draw(canvas: &mut Canvas, filler: &mut Filler, img: &ImageDraw<'_>) -> bool {
    let pic = img.pic;
    if pic.w == 0 || pic.h == 0 || img.clip.len() < 3 || pic.rgba.len() < pic.w as usize * pic.h as usize * 4 { return false; }
    let inv = img.m.inverse();
    if !inv.0.iter().all(|v| v.is_finite()) || img.m.det().abs() < 1e-12 { return false; }
    if !img.gray && !pic.gray && canvas.fmt != Format::Rgba { canvas.promote(); }
    // samples per device pixel and axis: more when the picture is shrunk
    let shrink = 1.0 / img.m.scale_factor();
    let n = (shrink.ceil() as usize).clamp(1, 4);
    let s = Sampler { img, inv, n, lut: img.lut(), fade: img.fade.min(100) as u32 * 255 / 100 };
    filler.begin(canvas.w as i32, canvas.h as i32);
    let c = img.clip;
    for i in 0..c.len() { let (a, b) = (c[i], c[(i + 1) % c.len()]); filler.edge(a[0], a[1], b[0], b[1]); }
    match canvas.fmt {
        Format::Gray => fill::<GrayPx>(canvas, filler, &s),
        Format::GrayAlpha => fill::<GrayAlphaPx>(canvas, filler, &s),
        Format::Rgba => fill::<RgbaPx>(canvas, filler, &s),
    }
    true
}

struct Sampler<'a> {
    img: &'a ImageDraw<'a>,
    inv: cadrender_core::geom::Aff,
    n: usize,
    lut: [u8; 256],
    /// 0..=255
    fade: u32,
}

impl Sampler<'_> {
    /// Premultiplied colour of device pixel (x, y).
    fn at(&self, x: i32, y: i32) -> [u8; 4] {
        let pic = self.img.pic;
        let (w, h) = (pic.w as i64, pic.h as i64);
        let mut acc = [0u32; 4];
        let k = self.n as f64;
        for j in 0..self.n {
            for i in 0..self.n {
                let (u, v) = self.inv.apply(x as f64 + (i as f64 + 0.5) / k, y as f64 + (j as f64 + 0.5) / k);
                let (iu, iv) = ((u.floor() as i64).clamp(0, w - 1), (v.floor() as i64).clamp(0, h - 1));
                let o = ((iv * w + iu) * 4) as usize;
                let p = &pic.rgba[o..o + 4];
                let a = p[3] as u32;
                for c in 0..3 { acc[c] += self.lut[p[c] as usize] as u32 * a; }
                acc[3] += a;
            }
        }
        let cnt = (self.n * self.n) as u32;
        if acc[3] == 0 { return [0; 4]; }
        // straight colour of the samples, then grey, then fade
        let mut rgb = [acc[0] / acc[3], acc[1] / acc[3], acc[2] / acc[3]];
        if self.img.gray { let g = (rgb[0] * 299 + rgb[1] * 587 + rgb[2] * 114) / 1000; rgb = [g, g, g]; }
        if self.fade > 0 {
            let bg = self.img.background;
            let bgc = [bg.r as u32, bg.g as u32, bg.b as u32];
            for c in 0..3 { rgb[c] = div255(rgb[c] * (255 - self.fade) + bgc[c] * self.fade); }
        }
        let a = acc[3] / cnt;
        [div255(rgb[0] * a) as u8, div255(rgb[1] * a) as u8, div255(rgb[2] * a) as u8, a as u8]
    }
}

fn fill<P: Px>(canvas: &mut Canvas, filler: &mut Filler, s: &Sampler<'_>) {
    let (w, h) = (canvas.w as i32, canvas.h as i32);
    let mut spans: Vec<(i32, i32, i32, u32)> = vec![];
    filler.sweep(FillRule::NonZero, |y, x0, x1, c| { if y >= 0 && y < h && c > 0 { spans.push((y, x0.max(0), x1.min(w), c)); } });
    for (y, x0, x1, cov) in spans {
        if x0 >= x1 { continue; }
        canvas.mark(x0 as u32, x1 as u32, y as u32);
        let row = y as usize * w as usize;
        for x in x0..x1 {
            let mut px = s.at(x, y);
            if P::BPP == 1 || P::BPP == 2 { px[1] = ((px[0] as u32 * 299 + px[1] as u32 * 587 + px[2] as u32 * 114) / 1000) as u8; }
            let i = (row + x as usize) * P::BPP;
            P::blend(&mut canvas.data[i..i + P::BPP], &px, cov.min(255));
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::RasterSink;
    use cadrender_core::color::Rgba;
    use cadrender_core::entity::image::Picture;
    use cadrender_core::geom::Aff;
    use cadrender_core::sink::{ImageDraw, Sink};

    fn pic() -> Picture {
        // 2 x 1: red, blue
        Picture { w: 2, h: 1, rgba: vec![255, 0, 0, 255, 0, 0, 255, 255], gray: false, encoded: vec![], mime: "image/png" }
    }
    fn draw<'a>(p: &'a Picture, clip: &'a [[f32; 2]], gray: bool, fade: u8) -> RasterSink {
        let mut s = RasterSink::page(&(), 20, 10, Rgba::WHITE);
        let d = ImageDraw { pic: p, m: Aff::scale(10.0, 10.0), clip, brightness: 50, contrast: 50, fade, gray, background: Rgba::WHITE };
        assert!(s.image(&d));
        s
    }

    #[test]
    fn picture_scaled_and_clipped() {
        let p = pic();
        let full = [[0.0, 0.0], [20.0, 0.0], [20.0, 10.0], [0.0, 10.0]];
        let s = draw(&p, &full, false, 0);
        assert_eq!(s.pixel(3, 5), Some([255, 0, 0, 255]));
        assert_eq!(s.pixel(15, 5), Some([0, 0, 255, 255]));
        // clip to the left half: the right half stays white
        let half = [[0.0, 0.0], [10.0, 0.0], [10.0, 10.0], [0.0, 10.0]];
        let s = draw(&p, &half, false, 0);
        assert_eq!(s.pixel(15, 5), Some([255, 255, 255, 255]));
        // monochrome: grey page stays grey, red becomes its brightness
        let s = draw(&p, &full, true, 0);
        assert_eq!(s.format(), crate::canvas::Format::Gray);
        assert_eq!(s.pixel(3, 5).unwrap()[0], 76);
        // fade 100: all page colour
        let s = draw(&p, &full, false, 100);
        assert_eq!(s.pixel(3, 5), Some([255, 255, 255, 255]));
    }
}
