use crate::blit::FastDrawPixmap;
use tiny_skia::IntSize;

// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
use crate::{cache::SvgrCache, render::TinySkiaPixmapMutExt};
use std::sync::Arc;

pub fn render(
    image: &usvgr::Image,
    transform: tiny_skia::Transform,
    pixmap: &mut tiny_skia::PixmapMut,
    cache: &mut crate::cache::SvgrCache,
    pixmap_pool: &crate::cache::PixmapPool,
) {
    if image.visibility() != usvgr::Visibility::Visible {
        return;
    }

    render_inner(
        image.kind(),
        image.view_box(),
        transform,
        image.rendering_mode(),
        pixmap,
        cache,
        pixmap_pool,
    );
}

pub fn render_inner(
    image_kind: &usvgr::ImageKind,
    view_box: usvgr::ViewBox,
    transform: tiny_skia::Transform,
    #[allow(unused_variables)] rendering_mode: usvgr::ImageRendering,
    pixmap: &mut tiny_skia::PixmapMut,
    cache: &mut crate::cache::SvgrCache,
    pixmap_pool: &crate::cache::PixmapPool,
) {
    match image_kind {
        usvgr::ImageKind::SVG {
            ref tree,
            ref original_href,
        } => {
            render_vector(
                tree,
                original_href,
                &view_box,
                transform,
                pixmap,
                cache,
                pixmap_pool,
            );
        }
        usvgr::ImageKind::DATA(ref data) => {
            draw_raster(data, view_box, rendering_mode, transform, pixmap, cache);
        }
    }
}

fn render_vector(
    tree: &usvgr::Tree,
    original_href: &str,
    view_box: &usvgr::ViewBox,
    transform: tiny_skia::Transform,
    pixmap: &mut tiny_skia::PixmapMut,
    cache: &mut crate::cache::SvgrCache,
    pixmap_pool: &crate::cache::PixmapPool,
) -> Option<()> {
    let sub_pixmap = cache.with_subpixmap_cache(
        &original_href,
        transform,
        pixmap_pool,
        IntSize::from_wh(pixmap.width(), pixmap.height()).unwrap(),
        |mut sub_pixmap, _| {
            let img_size = tree.size().to_int_size();
            let (ts, clip) = crate::geom::view_box_to_transform_with_clip(view_box, img_size);

            let source_transform = transform;
            let transform = transform.pre_concat(ts);
            let ctx = crate::render::Context::new_from_pixmap(&sub_pixmap);

            let pixmap_mut = &mut sub_pixmap.as_mut();
            crate::render(
                tree,
                transform,
                pixmap_mut,
                &mut SvgrCache::none(),
                pixmap_pool,
                &ctx,
            );

            if let Some(mask) =
                clip.and_then(|clip| pixmap_mut.create_rect_mask(source_transform, clip.to_rect()))
            {
                pixmap_mut.apply_mask(&mask);
            }

            Some(sub_pixmap)
        },
    )?;

    pixmap.fast_draw_pixmap(
        0,
        0,
        sub_pixmap.as_ref(),
        &tiny_skia::PixmapPaint::default(),
        tiny_skia::Transform::identity(),
        None,
    );

    Some(())
}

/// Calculates an image rect depending on the provided view box.
fn image_rect(view_box: &usvgr::ViewBox, img_size: tiny_skia::IntSize) -> tiny_skia::NonZeroRect {
    let new_size = crate::geom::fit_view_box(img_size.to_size(), view_box);
    let (x, y) = usvgr::utils::aligned_pos(
        view_box.aspect.align,
        view_box.rect.x(),
        view_box.rect.y(),
        view_box.rect.width() - new_size.width(),
        view_box.rect.height() - new_size.height(),
    );

    new_size.to_non_zero_rect(x, y)
}

pub fn draw_raster(
    img: &Arc<usvgr::PreloadedImageData>,
    view_box: usvgr::ViewBox,
    rendering_mode: usvgr::ImageRendering,
    transform: tiny_skia::Transform,
    pixmap: &mut tiny_skia::PixmapMut,
    cache: &mut SvgrCache,
) -> Option<()> {
    let img_bytes = img.data.clone();
    let raster = tiny_skia::PixmapRef::from_bytes(&img_bytes, img.width, img.height)?;

    let img_size = tiny_skia::IntSize::from_wh(raster.width(), raster.height())?;
    let rect = image_rect(&view_box, img_size);

    let ts = tiny_skia::Transform::from_row(
        rect.width() / raster.width() as f32,
        0.0,
        0.0,
        rect.height() / raster.height() as f32,
        rect.x(),
        rect.y(),
    );

    let mut quality = tiny_skia::FilterQuality::Bicubic;
    if rendering_mode == usvgr::ImageRendering::OptimizeSpeed {
        quality = tiny_skia::FilterQuality::Nearest;
    }

    let pattern = tiny_skia::Pattern::new(raster, tiny_skia::SpreadMode::Pad, quality, 1.0, ts);
    let mut paint = tiny_skia::Paint::default();
    paint.shader = pattern;

    if let Some(images) = cache.images.as_mut() {
        if draw_resampled(
            img, &view_box, rect, ts, quality, &paint, transform, pixmap, images,
        )
        .is_some()
        {
            return Some(());
        }
    }

    let mask = if view_box.aspect.slice {
        pixmap.create_rect_mask(transform, view_box.rect.to_rect())
    } else {
        None
    };

    pixmap.fill_rect(rect.to_rect(), &paint, transform, mask.as_ref());

    Some(())
}

/// Draws an image that is drawn at the same size and sub-pixel position as before from
/// its resampled copy. Returns `None` when the image has to be drawn directly: its
/// transform rotates or skews it, it is not on the canvas or it was not drawn like this
/// recently.
#[allow(clippy::too_many_arguments)]
fn draw_resampled(
    img: &Arc<usvgr::PreloadedImageData>,
    view_box: &usvgr::ViewBox,
    rect: tiny_skia::NonZeroRect,
    ts: tiny_skia::Transform,
    quality: tiny_skia::FilterQuality,
    paint: &tiny_skia::Paint,
    transform: tiny_skia::Transform,
    pixmap: &mut tiny_skia::PixmapMut,
    images: &mut crate::cache::ImageResampleCache,
) -> Option<()> {
    use std::hash::{Hash, Hasher};

    if transform.kx != 0.0 || transform.ky != 0.0 || transform.sx <= 0.0 || transform.sy <= 0.0 {
        return None;
    }

    let clip = view_box.aspect.slice.then(|| view_box.rect.to_rect());
    let mut bounds = rect.to_rect().transform(transform)?;
    if let Some(clip) = clip {
        bounds = bounds.intersect(&clip.transform(transform)?)?;
    }

    // Only the part on the canvas is resampled.
    let left = bounds.left().floor().max(0.0);
    let top = bounds.top().floor().max(0.0);
    let right = bounds.right().ceil().min(pixmap.width() as f32);
    let bottom = bounds.bottom().ceil().min(pixmap.height() as f32);
    if left >= right || top >= bottom {
        return None;
    }
    let width = (right - left) as u32;
    let height = (bottom - top) as u32;

    // The resampled pixels only depend on where the image lands relative to a whole pixel.
    let local = tiny_skia::Transform::from_translate(-left, -top).pre_concat(transform);

    let mut hasher = usvgr::ahash::AHasher::default();
    img.id.hash(&mut hasher);
    (
        Arc::as_ptr(img) as usize,
        img.data.len(),
        img.width,
        img.height,
    )
        .hash(&mut hasher);
    (width, height, quality as u8, view_box.aspect.slice).hash(&mut hasher);
    for value in [
        local.sx, local.sy, local.tx, local.ty, ts.sx, ts.sy, ts.tx, ts.ty,
    ] {
        value.to_bits().hash(&mut hasher);
    }
    if let Some(clip) = clip {
        for value in [clip.x(), clip.y(), clip.width(), clip.height()] {
            value.to_bits().hash(&mut hasher);
        }
    }
    let key = hasher.finish();

    if images.get(key).is_none() {
        if !images.seen_before(key) {
            return None;
        }

        let mut resampled = tiny_skia::Pixmap::new(width, height)?;
        let mut resampled_mut = resampled.as_mut();
        let mask = clip.and_then(|clip| resampled_mut.create_rect_mask(local, clip));
        resampled_mut.fill_rect(rect.to_rect(), paint, local, mask.as_ref());
        images.insert(key, resampled);
    }
    let resampled = images.get(key)?;

    pixmap.fast_draw_pixmap(
        left as i32,
        top as i32,
        resampled.as_ref(),
        &tiny_skia::PixmapPaint::default(),
        tiny_skia::Transform::identity(),
        None,
    );

    Some(())
}
