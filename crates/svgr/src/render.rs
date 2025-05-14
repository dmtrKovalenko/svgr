// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

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
    pixmap_pool: &crate::cache::PixmapPool,
) {
    for node in parent.children() {
        render_node(node, ctx, transform, pixmap, cache, pixmap_pool);
    }
}

pub fn render_node(
    node: &usvgr::Node,
    ctx: &Context,
    transform: tiny_skia::Transform,
    pixmap: &mut tiny_skia::PixmapMut,
    cache: &mut crate::cache::SvgrCache,
    pixmap_pool: &crate::cache::PixmapPool,
) {
    match node {
        usvgr::Node::Group(ref group) => {
            render_group(group, ctx, transform, pixmap, cache, pixmap_pool);
        }
        usvgr::Node::Path(ref path) => {
            crate::path::render(
                path,
                tiny_skia::BlendMode::SourceOver,
                ctx,
                transform,
                pixmap,
                cache,
                pixmap_pool,
            );
        }
        usvgr::Node::Image(ref image) => {
            crate::image::render(image, transform, pixmap, cache, pixmap_pool);
        }
        usvgr::Node::Text(ref text) => {
            render_group(text.flattened(), ctx, transform, pixmap, cache, pixmap_pool);
        }
    }
}

fn render_group(
    group: &usvgr::Group,
    ctx: &Context,
    transform: tiny_skia::Transform,
    pixmap: &mut tiny_skia::PixmapMut,
    cache: &mut crate::cache::SvgrCache,
    pixmap_pool: &crate::cache::PixmapPool,
) -> Option<()> {
    let final_transform = transform.pre_concat(group.transform());

    if !group.should_isolate() {
        // Tier 0: Non-isolated groups - Render Directly
        render_nodes(group, ctx, final_transform, pixmap, cache, pixmap_pool);
    } else {
        // Calculate the bounding box of the group's content *after* the final transform.
        let final_bbox = group.layer_bounding_box().transform(final_transform)?;
        let mut ibbox = if group.filters().is_empty() {
            tiny_skia::IntRect::from_xywh(
                final_bbox.x().floor() as i32 - 2,
                final_bbox.y().floor() as i32 - 2,
                final_bbox.width().ceil() as u32 + 4,
                final_bbox.height().ceil() as u32 + 4,
            )?
        } else {
            final_bbox.to_int_rect()
        };
        ibbox = crate::geom::fit_to_rect(ibbox, ctx.max_bbox)?;

        // Check if the isolated group has any effects (filters, clip path, mask)
        let require_transform_before_subsampling = !group.filters().is_empty()
            || group.clip_path().is_some()
            || group.mask().is_some()
            || !group.abs_transform().is_identity();

        if require_transform_before_subsampling {
            // Tier 2: Isolated group which requires transform before filtering or clipping
            render_isolated_group_with_effects(
                group,
                ctx,
                final_transform,
                ibbox, // Pass the corrected ibbox
                pixmap,
                cache,
                pixmap_pool,
            )?;
        } else {
            // Tier 1: Isolated Group with a transform but no filters/clip-path/masks which
            // allows to render the group elements without a transform and apply transform
            // only when we render the pixmap on a canvas significatnly increasing the caching rate
            render_isolated_group_without_effects(
                group,
                ctx,
                transform,
                ibbox,
                pixmap,
                cache,
                pixmap_pool,
            )?;
        }
    }

    Some(())
}

fn render_isolated_group_with_effects(
    group: &usvgr::Group,
    ctx: &Context,
    final_transform: tiny_skia::Transform,
    ibbox: tiny_skia::IntRect,
    pixmap: &mut tiny_skia::PixmapMut,
    cache: &mut crate::cache::SvgrCache,
    pixmap_pool: &crate::cache::PixmapPool,
) -> Option<()> {
    // Tier 2: Isolated Group *WITH* Filters, Clip Paths, or Masks
    // Cache key must include the final_transform for correctness.
    // Rendering into sub-pixmap uses final_transform relative to ibbox origin.
    // Effects are applied inside the sub-pixmap using this same relative transform.
    // Final draw is identity relative to ibbox position.

    let render_transform_in_subpixmap =
        tiny_skia::Transform::from_translate(-(ibbox.x() as f32), -(ibbox.y() as f32))
            .pre_concat(final_transform); // Use final_transform here

    let sub_pixmap = cache.with_subpixmap_cache(
        group,
        final_transform, // cache is invalidated every time the transform changed
        pixmap_pool,
        ibbox.size(),
        |mut sub_pixmap, cache| {
            render_nodes(
                group,
                ctx,
                render_transform_in_subpixmap,
                &mut sub_pixmap.as_mut(),
                cache,
                pixmap_pool,
            );

            if !group.filters().is_empty() {
                for filter in group.filters() {
                    crate::filter::apply(
                        filter,
                        render_transform_in_subpixmap,
                        &mut sub_pixmap,
                        cache,
                        pixmap_pool,
                    );
                }
            };

            if let Some(clip_path) = group.clip_path() {
                crate::clip::apply(
                    clip_path,
                    render_transform_in_subpixmap,
                    &mut sub_pixmap,
                    cache,
                    pixmap_pool,
                );
            }

            if let Some(mask) = group.mask() {
                crate::mask::apply(
                    mask,
                    ctx,
                    render_transform_in_subpixmap,
                    &mut sub_pixmap,
                    cache,
                    pixmap_pool,
                );
            }

            Some(sub_pixmap)
        },
    )?;

    let paint = tiny_skia::PixmapPaint {
        opacity: group.opacity().get(),
        blend_mode: convert_blend_mode(group.blend_mode()),
        quality: tiny_skia::FilterQuality::Bilinear,
    };

    pixmap.draw_pixmap(
        ibbox.x(),
        ibbox.y(),
        sub_pixmap.as_ref(),
        &paint,
        tiny_skia::Transform::identity(), // Already fully transformed/positioned in sub-pixmap
        None,
    );

    Some(())
}

fn render_isolated_group_without_effects(
    group: &usvgr::Group,
    ctx: &Context,
    transform: tiny_skia::Transform,
    ibbox: tiny_skia::IntRect,
    pixmap: &mut tiny_skia::PixmapMut,
    cache: &mut crate::cache::SvgrCache,
    pixmap_pool: &crate::cache::PixmapPool,
) -> Option<()> {
    let sub_pixmap = cache.with_subpixmap_cache(
        group,
        // Transform cache key is based only on the parent transform (if present)
        // which allows to effectively cache the group without transform at all and apply
        // transform only when finally rendering pixmap on canvas
        transform,
        pixmap_pool,
        ibbox.size(),
        |mut sub_pixmap, cache| {
            let render_transform_in_subpixmap =
                tiny_skia::Transform::from_translate(-(ibbox.x() as f32), -(ibbox.y() as f32))
                    .pre_concat(transform); // Render with parent transform + shift to the ibbox

            render_nodes(
                group,
                ctx,
                render_transform_in_subpixmap,
                &mut sub_pixmap.as_mut(),
                cache,
                pixmap_pool,
            );
            Some(sub_pixmap)
        },
    )?;

    let paint = tiny_skia::PixmapPaint {
        opacity: group.opacity().get(),
        blend_mode: convert_blend_mode(group.blend_mode()),
        quality: tiny_skia::FilterQuality::Bilinear,
    };

    pixmap.draw_pixmap(
        ibbox.x(), // Draw at the ibbox origin
        ibbox.y(), // Draw at the ibbox origin
        sub_pixmap.as_ref(),
        &paint,
        // Finally applying the group transform when rendering either new or cached pixmap
        group.transform(),
        None,
    );

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
