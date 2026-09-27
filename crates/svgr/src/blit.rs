//! Integer offset pixmap compositing.
//!
//! Layers, cached groups and filter results are drawn back onto the canvas at whole pixel
//! positions. tiny-skia runs that through its floating point pipeline (its nearest-neighbour
//! `gather` stage only exists there), which made compositing a full-frame layer one of the
//! most expensive parts of a frame. Source-over at an integer offset is done here on 8-bit
//! premultiplied pixels instead; everything else still goes through tiny-skia.

use tiny_skia::{BlendMode, Mask, PixmapMut, PixmapPaint, PixmapRef, Transform};

pub(crate) trait FastDrawPixmap {
    /// Same as `draw_pixmap`, faster for source-over without a transform or a mask.
    fn fast_draw_pixmap(
        &mut self,
        x: i32,
        y: i32,
        src: PixmapRef,
        paint: &PixmapPaint,
        transform: Transform,
        mask: Option<&Mask>,
    );
}

impl FastDrawPixmap for PixmapMut<'_> {
    fn fast_draw_pixmap(
        &mut self,
        x: i32,
        y: i32,
        src: PixmapRef,
        paint: &PixmapPaint,
        transform: Transform,
        mask: Option<&Mask>,
    ) {
        // tiny-skia repeats the edge of a pixmap drawn at a negative offset, keep that
        if transform.is_identity()
            && mask.is_none()
            && paint.blend_mode == BlendMode::SourceOver
            && x >= 0
            && y >= 0
        {
            source_over(self, x, y, src, paint.opacity);
        } else {
            self.draw_pixmap(x, y, src, paint, transform, mask);
        }
    }
}

impl FastDrawPixmap for tiny_skia::Pixmap {
    fn fast_draw_pixmap(
        &mut self,
        x: i32,
        y: i32,
        src: PixmapRef,
        paint: &PixmapPaint,
        transform: Transform,
        mask: Option<&Mask>,
    ) {
        self.as_mut()
            .fast_draw_pixmap(x, y, src, paint, transform, mask);
    }
}

/// `v / 255` rounded to the nearest integer, exact for `v <= 255 * 255`.
#[inline(always)]
fn div255(v: u32) -> u32 {
    let v = v + 128;
    (v + (v >> 8)) >> 8
}

fn source_over(dst: &mut PixmapMut, x: i32, y: i32, src: PixmapRef, opacity: f32) {
    let opacity = (opacity.clamp(0.0, 1.0) * 255.0 + 0.5) as u32;
    if opacity == 0 {
        return;
    }

    let (dst_width, dst_height) = (dst.width() as i32, dst.height() as i32);
    let (src_width, src_height) = (src.width() as i32, src.height() as i32);
    let left = x.max(0);
    let top = y.max(0);
    let right = (x + src_width).min(dst_width);
    let bottom = (y + src_height).min(dst_height);
    if left >= right || top >= bottom {
        return;
    }

    let row_len = ((right - left) * 4) as usize;
    let src_data = src.data();
    let dst_data = dst.data_mut();

    for row in top..bottom {
        let src_start = (((row - y) * src_width + (left - x)) * 4) as usize;
        let dst_start = ((row * dst_width + left) * 4) as usize;
        let src_row = &src_data[src_start..src_start + row_len];
        let dst_row = &mut dst_data[dst_start..dst_start + row_len];

        for (s, d) in src_row.chunks_exact(4).zip(dst_row.chunks_exact_mut(4)) {
            let (mut r, mut g, mut b, mut a) = (s[0] as u32, s[1] as u32, s[2] as u32, s[3] as u32);
            if opacity != 255 {
                r = div255(r * opacity);
                g = div255(g * opacity);
                b = div255(b * opacity);
                a = div255(a * opacity);
            }

            match a {
                0 => {}
                255 => {
                    d[0] = r as u8;
                    d[1] = g as u8;
                    d[2] = b as u8;
                    d[3] = 255;
                }
                _ => {
                    let inv = 255 - a;
                    d[0] = (r + div255(d[0] as u32 * inv)) as u8;
                    d[1] = (g + div255(d[1] as u32 * inv)) as u8;
                    d[2] = (b + div255(d[2] as u32 * inv)) as u8;
                    d[3] = (a + div255(d[3] as u32 * inv)) as u8;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tiny_skia::Pixmap;

    fn random_premultiplied(width: u32, height: u32, seed: &mut u32) -> Pixmap {
        let mut pixmap = Pixmap::new(width, height).unwrap();
        for pixel in pixmap.data_mut().chunks_exact_mut(4) {
            *seed ^= *seed << 13;
            *seed ^= *seed >> 17;
            *seed ^= *seed << 5;
            let alpha = match *seed % 4 {
                0 => 0,
                1 => 255,
                _ => (*seed >> 8) as u8,
            };
            for (channel, value) in pixel[..3].iter_mut().enumerate() {
                *value = ((*seed >> (channel * 7)) as u8).min(alpha);
            }
            pixel[3] = alpha;
        }
        pixmap
    }

    #[test]
    fn matches_tiny_skia_within_rounding() {
        let mut seed = 0xDEAD_BEEF;
        for (x, y, opacity) in [
            (0, 0, 1.0),
            (7, 3, 1.0),
            (5, 9, 0.5),
            (30, 20, 0.33),
            (40, 40, 0.9),
            (60, 0, 1.0),
        ] {
            let src = random_premultiplied(37, 29, &mut seed);
            let dst = random_premultiplied(48, 40, &mut seed);
            let paint = PixmapPaint {
                opacity,
                ..Default::default()
            };

            let mut expected = dst.clone();
            expected.draw_pixmap(x, y, src.as_ref(), &paint, Transform::identity(), None);
            let mut actual = dst.clone();
            actual.fast_draw_pixmap(x, y, src.as_ref(), &paint, Transform::identity(), None);

            for (index, (e, a)) in expected.data().iter().zip(actual.data()).enumerate() {
                let pixel = index / 4;
                assert!(
                    e.abs_diff(*a) <= 1,
                    "{e} vs {a} at offset ({x}, {y}) opacity {opacity}, pixel ({}, {}) channel {}, dst {:?}",
                    pixel % 48,
                    pixel / 48,
                    index % 4,
                    &dst.data()[pixel * 4..pixel * 4 + 4],
                );
            }
        }
    }
}
