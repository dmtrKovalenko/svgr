// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use std::sync::Arc;

use crate::render::TinySkiaPixmapMutExt;

pub fn render(
    image: &usvgr::Image,
    transform: tiny_skia::Transform,
    pixmap: &mut tiny_skia::PixmapMut,
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
    );
}

pub fn render_inner(
    image_kind: &usvgr::ImageKind,
    view_box: usvgr::ViewBox,
    transform: tiny_skia::Transform,
    #[allow(unused_variables)] rendering_mode: usvgr::ImageRendering,
    pixmap: &mut tiny_skia::PixmapMut,
) {
    match image_kind {
        usvgr::ImageKind::SVG(ref tree) => {
            render_vector(tree, &view_box, transform, pixmap);
        }
        usvgr::ImageKind::DATA(ref data) => {
            draw_raster(data, view_box, rendering_mode, transform, pixmap);
        }
    }
}

fn render_vector(
    tree: &usvgr::Tree,
    view_box: &usvgr::ViewBox,
    transform: tiny_skia::Transform,
    pixmap: &mut tiny_skia::PixmapMut,
) -> Option<()> {
    let img_size = tree.size().to_int_size();
    let (ts, clip) = crate::geom::view_box_to_transform_with_clip(view_box, img_size);

    let mut sub_pixmap = tiny_skia::Pixmap::new(pixmap.width(), pixmap.height()).unwrap();

    let source_transform = transform;
    let transform = transform.pre_concat(ts);

    crate::render(tree, transform, &mut sub_pixmap.as_mut());

    let mask = if let Some(clip) = clip {
        pixmap.create_rect_mask(source_transform, clip.to_rect())
    } else {
        None
    };

    pixmap.draw_pixmap(
        0,
        0,
        sub_pixmap.as_ref(),
        &tiny_skia::PixmapPaint::default(),
        tiny_skia::Transform::identity(),
        mask.as_ref(),
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
) -> Option<()> {
    let img_bytes = img.data.clone();
    let raster = tiny_skia::PixmapRef::from_bytes(
        &img_bytes,
        img.size.width() as u32,
        img.size.height() as u32,
    )?;

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

    let mask = if view_box.aspect.slice {
        pixmap.create_rect_mask(transform, view_box.rect.to_rect())
    } else {
        None
    };

    pixmap.fill_rect(rect.to_rect(), &paint, transform, mask.as_ref());

    Some(())
}
