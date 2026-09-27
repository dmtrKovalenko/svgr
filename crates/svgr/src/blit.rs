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

/// `u8 as f32 * (1.0 / 255.0)` like tiny-skia loads a channel.
static UNIT: [f32; 256] = {
    let mut table = [0.0; 256];
    let mut i = 0;
    while i < 256 {
        table[i] = i as f32 * (1.0 / 255.0);
        i += 1;
    }
    table
};

/// Stores a channel like tiny-skia: clamped to 0..1, scaled and rounded half to even.
#[inline(always)]
fn unit_to_u8(value: f32) -> u8 {
    (value.clamp(0.0, 1.0) * 255.0).round_ties_even() as u8
}

/// Source-over with exactly the arithmetic of tiny-skia's floating point pipeline, so the
/// result is identical to `draw_pixmap`, just without its per-pixel stage machinery.
fn source_over(dst: &mut PixmapMut, x: i32, y: i32, src: PixmapRef, opacity: f32) {
    let opacity = opacity.clamp(0.0, 1.0);
    if opacity == 0.0 {
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
            // a transparent source leaves the destination as is, an opaque one replaces it
            // (both exact in the floating point pipeline, see the tests)
            if s[3] == 0 {
                continue;
            }
            if s[3] == 255 && opacity == 1.0 {
                d.copy_from_slice(s);
                continue;
            }

            let mut source = [
                UNIT[s[0] as usize],
                UNIT[s[1] as usize],
                UNIT[s[2] as usize],
                UNIT[s[3] as usize],
            ];
            if opacity != 1.0 {
                for channel in &mut source {
                    *channel *= opacity;
                }
            }

            let inv_alpha = 1.0 - source[3];
            for channel in 0..4 {
                d[channel] = unit_to_u8(UNIT[d[channel] as usize] * inv_alpha + source[channel]);
            }
        }
    }
}

/// Source-over of `src` placed at the fractional position (`x`, `y`) with bilinear
/// filtering. The offset is the same for every pixel, so are the four filter weights.
pub(crate) fn source_over_translated(
    dst: &mut PixmapMut,
    x: f32,
    y: f32,
    src: PixmapRef,
    opacity: f32,
) {
    let (left, top) = (x.floor(), y.floor());
    let (fx, fy) = (x - left, y - top);
    if fx == 0.0 && fy == 0.0 && left >= 0.0 && top >= 0.0 {
        source_over(dst, left as i32, top as i32, src, opacity);
        return;
    }

    let opacity = (opacity.clamp(0.0, 1.0) * 255.0 + 0.5) as u32;
    if opacity == 0 {
        return;
    }

    // 8 bit weights of the source pixel to the left/top and to the right/bottom
    let wx = (fx * 256.0 + 0.5) as u32;
    let wy = (fy * 256.0 + 0.5) as u32;
    let weights = [
        (256 - wx) * (256 - wy),
        wx * (256 - wy),
        (256 - wx) * wy,
        wx * wy,
    ];

    let (left, top) = (left as i32, top as i32);
    let (src_width, src_height) = (src.width() as i32, src.height() as i32);
    let (dst_width, dst_height) = (dst.width() as i32, dst.height() as i32);
    let src_data = src.data();
    let dst_data = dst.data_mut();

    // transparent outside of the source
    let pixel = |sx: i32, sy: i32| -> [u32; 4] {
        if sx < 0 || sy < 0 || sx >= src_width || sy >= src_height {
            return [0; 4];
        }
        let index = ((sy * src_width + sx) * 4) as usize;
        [
            src_data[index] as u32,
            src_data[index + 1] as u32,
            src_data[index + 2] as u32,
            src_data[index + 3] as u32,
        ]
    };

    // with a fractional offset the source covers one more row and column
    for row in top.max(0)..(top + src_height + 1).min(dst_height) {
        let sy = row - top;
        for column in left.max(0)..(left + src_width + 1).min(dst_width) {
            let sx = column - left;
            // the destination pixel mixes the source pixels at (sx - 1, sy - 1) .. (sx, sy)
            let samples = [
                pixel(sx, sy),
                pixel(sx - 1, sy),
                pixel(sx, sy - 1),
                pixel(sx - 1, sy - 1),
            ];

            let mut color = [0u32; 4];
            for (sample, weight) in samples.iter().zip(weights) {
                for channel in 0..4 {
                    color[channel] += sample[channel] * weight;
                }
            }
            // 16 bit weights sum to 65536
            let mut color = color.map(|c| (c + 32768) >> 16);
            if opacity != 255 {
                color = color.map(|c| div255(c * opacity));
            }

            let a = color[3];
            if a == 0 {
                continue;
            }
            let index = ((row * dst_width + column) * 4) as usize;
            let d = &mut dst_data[index..index + 4];
            let inv = 255 - a;
            for channel in 0..4 {
                d[channel] = (color[channel] + div255(d[channel] as u32 * inv)).min(255) as u8;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn load_store_round_trips() {
        for value in 0..=255u8 {
            assert_eq!(unit_to_u8(UNIT[value as usize]), value);
            assert_eq!(unit_to_u8(UNIT[value as usize] * 1.0 + 0.0), value);
        }
    }
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
    fn matches_tiny_skia_exactly() {
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
                    e == a,
                    "{e} vs {a} at offset ({x}, {y}) opacity {opacity}, pixel ({}, {}) channel {}, dst {:?}",
                    pixel % 48,
                    pixel / 48,
                    index % 4,
                    &dst.data()[pixel * 4..pixel * 4 + 4],
                );
            }
        }
    }

    #[test]
    fn translated_blit_matches_bilinear_reference() {
        let mut seed = 0xC0FF_EE11;
        let src = random_premultiplied(9, 7, &mut seed);
        for (x, y) in [(3.25f32, 2.5f32), (0.75, 0.0), (10.0, 4.5), (-2.5, 1.125)] {
            let dst = random_premultiplied(24, 16, &mut seed);
            let mut actual = dst.clone();
            source_over_translated(&mut actual.as_mut(), x, y, src.as_ref(), 1.0);

            for row in 0..16 {
                for column in 0..24 {
                    // bilinear sample of the source at the pixel center
                    let u = column as f32 + 0.5 - x - 0.5;
                    let v = row as f32 + 0.5 - y - 0.5;
                    let (u0, v0) = (u.floor(), v.floor());
                    let (fu, fv) = (u - u0, v - v0);
                    let get = |sx: f32, sy: f32, c: usize| -> f32 {
                        if sx < 0.0 || sy < 0.0 || sx >= 9.0 || sy >= 7.0 {
                            0.0
                        } else {
                            src.data()[((sy as usize) * 9 + sx as usize) * 4 + c] as f32
                        }
                    };
                    let sample = |c: usize| {
                        get(u0, v0, c) * (1.0 - fu) * (1.0 - fv)
                            + get(u0 + 1.0, v0, c) * fu * (1.0 - fv)
                            + get(u0, v0 + 1.0, c) * (1.0 - fu) * fv
                            + get(u0 + 1.0, v0 + 1.0, c) * fu * fv
                    };
                    let a = sample(3);
                    for c in 0..4 {
                        let d = dst.data()[(row * 24 + column) * 4 + c] as f32;
                        let expected = sample(c) + d * (1.0 - a / 255.0);
                        let got = actual.data()[(row * 24 + column) * 4 + c] as f32;
                        assert!(
                            (expected - got).abs() <= 2.0,
                            "{expected} vs {got} at ({column}, {row}) offset ({x}, {y})"
                        );
                    }
                }
            }
        }
    }
}
