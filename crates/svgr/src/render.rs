// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use crate::OptionLog;

/// General context for the rendering.
pub struct Context {
    /// The max bounding box for the whole SVG.
    pub max_bbox: tiny_skia::IntRect,
}

impl Context {
    /// Default implementation of the max bounding box is 4 times the size of the pixmap.
    pub fn new_from_pixmap(pixmap: &tiny_skia::Pixmap) -> Self {
        let target_size = tiny_skia::IntSize::from_wh(pixmap.width(), pixmap.height()).unwrap();
        let max_bbox = tiny_skia::IntRect::from_xywh(
            -(target_size.width() as i32) * 2,
            -(target_size.height() as i32) * 2,
            target_size.width() * 4,
            target_size.height() * 4,
        )
        .unwrap();

        Self { max_bbox }
    }

    /// Unsafe but faster max bbox which might cut some filters and masks.
    pub fn new_from_pixmap_unsafe(pixmap: &tiny_skia::Pixmap) -> Self {
        let max_bbox =
            tiny_skia::IntRect::from_xywh(0, 0, pixmap.width(), pixmap.height()).unwrap();

        Self { max_bbox }
    }
}

pub fn render_nodes(
    parent: &usvgr::Group,
    ctx: &Context,
    transform: tiny_skia::Transform,
    pixmap: &mut tiny_skia::PixmapMut,
    cache: &mut crate::cache::SvgrCache,
) {
    for node in parent.children() {
        render_node(node, ctx, transform, pixmap, cache);
    }
}

pub fn render_node(
    node: &usvgr::Node,
    ctx: &Context,
    transform: tiny_skia::Transform,
    pixmap: &mut tiny_skia::PixmapMut,
    cache: &mut crate::cache::SvgrCache,
) {
    match node {
        usvgr::Node::Group(ref group) => {
            render_group(group, ctx, transform, pixmap, cache);
        }
        usvgr::Node::Path(ref path) => {
            crate::path::render(
                path,
                tiny_skia::BlendMode::SourceOver,
                ctx,
                transform,
                pixmap,
                cache,
            );
        }
        usvgr::Node::Image(ref image) => {
            crate::image::render(image, transform, pixmap, cache);
        }
        usvgr::Node::Text(ref text) => {
            render_group(text.flattened(), ctx, transform, pixmap, cache);
        }
    }
}

fn render_group(
    group: &usvgr::Group,
    ctx: &Context,
    transform: tiny_skia::Transform,
    pixmap: &mut tiny_skia::PixmapMut,
    cache: &mut crate::cache::SvgrCache,
) -> Option<()> {
    let transform = transform.pre_concat(group.transform());

    if !group.should_isolate() {
        render_nodes(group, ctx, transform, pixmap, cache);
    } else {
        let bbox = group.layer_bounding_box().transform(transform)?;
        let mut ibbox = if group.filters().is_empty() {
            // Convert group bbox into an integer one, expanding each side outwards by 2px
            // to make sure that anti-aliased pixels would not be clipped.
            tiny_skia::IntRect::from_xywh(
                bbox.x().floor() as i32 - 2,
                bbox.y().floor() as i32 - 2,
                bbox.width().ceil() as u32 + 4,
                bbox.height().ceil() as u32 + 4,
            )?
            // The bounding box for groups with filters is special and should not be expanded by 2px,
        } else {
            // because it's already acting as a clipping region.
            let bbox = bbox.to_int_rect();
            // Make sure our filter region is not bigger than 4x the canvas size.
            // This is required mainly to prevent huge filter regions that would tank the performance.
            // It should not affect the final result in any way.
            crate::geom::fit_to_rect(bbox, ctx.max_bbox)?
        };

        let sub_pixmap = cache.with_subpixmap_cache(group, |cache| {
            // Make sure our layer is not bigger than 4x the canvas size.
            // This is required to prevent huge layers.
            if group.filters().is_empty() {
                ibbox = crate::geom::fit_to_rect(ibbox, ctx.max_bbox)?;
            }

            let shift_ts = {
                // Original shift.
                let mut dx = bbox.x();
                let mut dy = bbox.y();

                // Account for subpixel positioned layers.
                dx -= bbox.x() - ibbox.x() as f32;
                dy -= bbox.y() - ibbox.y() as f32;

                tiny_skia::Transform::from_translate(-dx, -dy)
            };

            let transform = shift_ts.pre_concat(transform);

            let mut sub_pixmap = tiny_skia::Pixmap::new(ibbox.width(), ibbox.height())
                .log_none(|| log::warn!("Failed to allocate a group layer for: {:?}.", ibbox))?;

            render_nodes(group, ctx, transform, &mut sub_pixmap.as_mut(), cache);

            if !group.filters().is_empty() {
                for filter in group.filters() {
                    crate::filter::apply(filter, transform, &mut sub_pixmap, cache);
                }
            }

            if let Some(clip_path) = group.clip_path() {
                crate::clip::apply(clip_path, transform, &mut sub_pixmap, cache);
            }

            if let Some(mask) = group.mask() {
                crate::mask::apply(mask, ctx, transform, &mut sub_pixmap, cache);
            }

            Some((sub_pixmap, cache))
        })?;

        let paint = tiny_skia::PixmapPaint {
            opacity: group.opacity().get(),
            blend_mode: convert_blend_mode(group.blend_mode()),
            quality: tiny_skia::FilterQuality::Nearest,
        };

        pixmap.draw_pixmap(
            ibbox.x(),
            ibbox.y(),
            sub_pixmap.as_ref().as_ref(),
            &paint,
            tiny_skia::Transform::identity(),
            None,
        );
    }

    Some(())
}

pub trait TinySkiaPixmapMutExt {
    fn create_rect_mask(
        &self,
        transform: tiny_skia::Transform,
        rect: tiny_skia::Rect,
    ) -> Option<tiny_skia::Mask>;
}

impl TinySkiaPixmapMutExt for tiny_skia::PixmapMut<'_> {
    fn create_rect_mask(
        &self,
        transform: tiny_skia::Transform,
        rect: tiny_skia::Rect,
    ) -> Option<tiny_skia::Mask> {
        let path = tiny_skia::PathBuilder::from_rect(rect);

        let mut mask = tiny_skia::Mask::new(self.width(), self.height())?;
        mask.fill_path(&path, tiny_skia::FillRule::Winding, true, transform);

        Some(mask)
    }
}

pub fn convert_blend_mode(mode: usvgr::BlendMode) -> tiny_skia::BlendMode {
    match mode {
        usvgr::BlendMode::Normal => tiny_skia::BlendMode::SourceOver,
        usvgr::BlendMode::Multiply => tiny_skia::BlendMode::Multiply,
        usvgr::BlendMode::Screen => tiny_skia::BlendMode::Screen,
        usvgr::BlendMode::Overlay => tiny_skia::BlendMode::Overlay,
        usvgr::BlendMode::Darken => tiny_skia::BlendMode::Darken,
        usvgr::BlendMode::Lighten => tiny_skia::BlendMode::Lighten,
        usvgr::BlendMode::ColorDodge => tiny_skia::BlendMode::ColorDodge,
        usvgr::BlendMode::ColorBurn => tiny_skia::BlendMode::ColorBurn,
        usvgr::BlendMode::HardLight => tiny_skia::BlendMode::HardLight,
        usvgr::BlendMode::SoftLight => tiny_skia::BlendMode::SoftLight,
        usvgr::BlendMode::Difference => tiny_skia::BlendMode::Difference,
        usvgr::BlendMode::Exclusion => tiny_skia::BlendMode::Exclusion,
        usvgr::BlendMode::Hue => tiny_skia::BlendMode::Hue,
        usvgr::BlendMode::Saturation => tiny_skia::BlendMode::Saturation,
        usvgr::BlendMode::Color => tiny_skia::BlendMode::Color,
        usvgr::BlendMode::Luminosity => tiny_skia::BlendMode::Luminosity,
    }
}
