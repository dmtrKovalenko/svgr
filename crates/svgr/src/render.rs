// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

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

    // Check if this is a static group that can benefit from caching
    if let Some(static_hash) = group.static_hash() {
        if cache.has_static_cache() {
            // Check if already cached
            if let Some(cached) = cache.get_static(static_hash) {
                draw_cached_static_group(group, cached, transform, pixmap);
                return Some(());
            }

            // Calculate bounding box to determine if caching is worthwhile
            let group_bbox = group.layer_bounding_box();
            let ibbox = tiny_skia::IntRect::from_xywh(
                group_bbox.x().floor() as i32 - 2,
                group_bbox.y().floor() as i32 - 2,
                group_bbox.width().ceil() as u32 + 4,
                group_bbox.height().ceil() as u32 + 4,
            )?;

            render_and_cache_static_group(
                group,
                static_hash,
                ctx,
                transform,
                ibbox,
                pixmap,
                cache,
                pixmap_pool,
            )?;

            return Some(());
        }
    }

    // Non-static or small group - use original rendering path
    if !group.should_isolate() {
        render_nodes(group, ctx, final_transform, pixmap, cache, pixmap_pool);
    } else {
        render_isolated_group(group, ctx, transform, pixmap, cache, pixmap_pool)?;
    }

    Some(())
}

/// Render a static group to a sub-pixmap and cache it
fn render_and_cache_static_group(
    group: &usvgr::Group,
    static_hash: u64,
    ctx: &Context,
    parent_transform: tiny_skia::Transform,
    ibbox: tiny_skia::IntRect,
    pixmap: &mut tiny_skia::PixmapMut,
    cache: &mut crate::cache::SvgrCache,
    pixmap_pool: &crate::cache::PixmapPool,
) -> Option<()> {
    // Allocate sub-pixmap for rendering
    let mut sub_pixmap = pixmap_pool.take_or_allocate(ibbox.width(), ibbox.height())?;

    // Render transform: translate content to fit in pixmap starting at (0,0)
    // and apply group's own transform
    let render_transform =
        tiny_skia::Transform::from_translate(-(ibbox.x() as f32), -(ibbox.y() as f32))
            .pre_concat(group.transform());

    // Render children to sub-pixmap
    render_nodes(
        group,
        ctx,
        render_transform,
        &mut sub_pixmap.as_mut(),
        cache,
        pixmap_pool,
    );

    // Apply group effects (filters, clip-path, mask) if any
    if group.should_isolate() {
        apply_group_effects(
            group,
            render_transform,
            &mut sub_pixmap,
            cache,
            ctx,
            pixmap_pool,
        );
    }

    // Store in static cache (permanent, no eviction)
    cache.insert_static(static_hash, sub_pixmap);

    // Draw from cache
    if let Some(cached) = cache.get_static(static_hash) {
        draw_cached_static_group(group, cached, parent_transform, pixmap);
    }

    Some(())
}

/// Draw a cached static group to the target pixmap
fn draw_cached_static_group(
    group: &usvgr::Group,
    cached: &tiny_skia::Pixmap,
    parent_transform: tiny_skia::Transform,
    pixmap: &mut tiny_skia::PixmapMut,
) {
    // For static groups, we rendered at identity with group transform applied,
    // so we just need to apply the parent transform
    let group_bbox = group.layer_bounding_box();
    let ibbox = tiny_skia::IntRect::from_xywh(
        group_bbox.x().floor() as i32 - 2,
        group_bbox.y().floor() as i32 - 2,
        group_bbox.width().ceil() as u32 + 4,
        group_bbox.height().ceil() as u32 + 4,
    );

    let Some(ibbox) = ibbox else { return };

    let paint = tiny_skia::PixmapPaint {
        opacity: group.opacity().get(),
        blend_mode: convert_blend_mode(group.blend_mode()),
        quality: tiny_skia::FilterQuality::Bilinear,
    };

    // Apply parent transform and translate to bounding box position
    let draw_transform = parent_transform.pre_translate(ibbox.x() as f32, ibbox.y() as f32);

    pixmap.draw_pixmap(0, 0, cached.as_ref(), &paint, draw_transform, None);
}

fn render_isolated_group(
    group: &usvgr::Group,
    ctx: &Context,
    parent_transform: tiny_skia::Transform,
    pixmap: &mut tiny_skia::PixmapMut,
    cache: &mut crate::cache::SvgrCache,
    pixmap_pool: &crate::cache::PixmapPool,
) -> Option<()> {
    // For LRU cache, we include transform in the key since we're caching at final transform
    let final_transform = parent_transform.pre_concat(group.transform());
    let final_bbox = group.layer_bounding_box().transform(final_transform)?;
    let final_ibbox = if group.filters().is_empty() {
        tiny_skia::IntRect::from_xywh(
            final_bbox.x().floor() as i32 - 2,
            final_bbox.y().floor() as i32 - 2,
            final_bbox.width().ceil() as u32 + 4,
            final_bbox.height().ceil() as u32 + 4,
        )?
    } else {
        final_bbox.to_int_rect()
    };
    let final_ibbox = crate::geom::fit_to_rect(final_ibbox, ctx.max_bbox)?;

    let render_transform_in_subpixmap =
        tiny_skia::Transform::from_translate(-(final_ibbox.x() as f32), -(final_ibbox.y() as f32))
            .pre_concat(final_transform);

    let sub_pixmap = cache.with_subpixmap_cache(
        group,
        final_transform,
        pixmap_pool,
        final_ibbox.size(),
        |mut sub_pixmap, cache| {
            render_nodes(
                group,
                ctx,
                render_transform_in_subpixmap,
                &mut sub_pixmap.as_mut(),
                cache,
                pixmap_pool,
            );

            apply_group_effects(
                group,
                render_transform_in_subpixmap,
                &mut sub_pixmap,
                cache,
                ctx,
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
        final_ibbox.x(),
        final_ibbox.y(),
        sub_pixmap.as_ref(),
        &paint,
        tiny_skia::Transform::identity(),
        None,
    );

    Some(())
}

/// Apply filters, clip-paths, and masks to a rendered group
fn apply_group_effects(
    group: &usvgr::Group,
    transform: tiny_skia::Transform,
    sub_pixmap: &mut tiny_skia::Pixmap,
    cache: &mut crate::cache::SvgrCache,
    ctx: &Context,
    pixmap_pool: &crate::cache::PixmapPool,
) {
    // Apply filters
    if !group.filters().is_empty() {
        for filter in group.filters() {
            crate::filter::apply(filter, transform, sub_pixmap, cache, pixmap_pool);
        }
    }

    // Apply clip path
    if let Some(clip_path) = group.clip_path() {
        crate::clip::apply(clip_path, transform, sub_pixmap, cache, pixmap_pool);
    }

    // Apply mask
    if let Some(mask) = group.mask() {
        crate::mask::apply(mask, ctx, transform, sub_pixmap, cache, pixmap_pool);
    }
}

pub(crate) fn convert_blend_mode(mode: usvgr::BlendMode) -> tiny_skia::BlendMode {
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
