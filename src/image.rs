// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use crate::{render::Canvas, ConvTransform};

pub fn draw(image: &usvgr::Image, canvas: &mut Canvas) -> usvgr::PathBbox {
    if image.visibility != usvgr::Visibility::Visible {
        return image.view_box.rect.to_path_bbox();
    }

    raster_images::draw_raster(&image.data, image.view_box, image.rendering_mode, canvas);
    image.view_box.rect.to_path_bbox()
}

pub(crate) mod raster_images {
    use crate::render::Canvas;
    use std::sync::Arc;
    use usvgr::PreloadedImageData;

    pub fn draw_raster(
        img: &Arc<PreloadedImageData>,
        view_box: usvgr::ViewBox,
        rendering_mode: usvgr::ImageRendering,
        canvas: &mut Canvas,
    ) -> Option<()> {
        let mut img_bytes = img.data.clone();
        let pixmap =
            tiny_skia::PixmapMut::from_bytes(img_bytes.as_mut_slice(), img.width, img.height)?;

        let mut filter = tiny_skia::FilterQuality::Bicubic;
        if rendering_mode == usvgr::ImageRendering::OptimizeSpeed {
            filter = tiny_skia::FilterQuality::Nearest;
        }

        let r = image_rect(&view_box, usvgr::ScreenSize::new(img.width, img.height)?);
        let rect = tiny_skia::Rect::from_xywh(
            r.x() as f32,
            r.y() as f32,
            r.width() as f32,
            r.height() as f32,
        )?;

        let ts = tiny_skia::Transform::from_row(
            rect.width() as f32 / pixmap.width() as f32,
            0.0,
            0.0,
            rect.height() as f32 / pixmap.height() as f32,
            r.x() as f32,
            r.y() as f32,
        );

        let pattern =
            tiny_skia::Pattern::new(pixmap.as_ref(), tiny_skia::SpreadMode::Pad, filter, 1.0, ts);
        let mut paint = tiny_skia::Paint::default();
        paint.shader = pattern;

        if view_box.aspect.slice {
            let r = view_box.rect;
            let rect = tiny_skia::Rect::from_xywh(
                r.x() as f32,
                r.y() as f32,
                r.width() as f32,
                r.height() as f32,
            )?;

            canvas.set_clip_rect(rect);
        }

        canvas
            .pixmap
            .fill_rect(rect, &paint, canvas.transform, canvas.clip.as_ref());
        canvas.clip = None;

        Some(())
    }

    /// Calculates an image rect depending on the provided view box.
    fn image_rect(view_box: &usvgr::ViewBox, img_size: usvgr::ScreenSize) -> usvgr::Rect {
        let new_size = img_size.to_size().fit_view_box(view_box);
        let (x, y) = usvgr::utils::aligned_pos(
            view_box.aspect.align,
            view_box.rect.x(),
            view_box.rect.y(),
            view_box.rect.width() - new_size.width(),
            view_box.rect.height() - new_size.height(),
        );

        new_size.to_rect(x, y)
    }
}
