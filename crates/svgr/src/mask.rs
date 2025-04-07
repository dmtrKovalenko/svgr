// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use tiny_skia::IntSize;

use crate::{render::Context, SvgrCache};

pub fn apply(
    mask: &usvgr::Mask,
    ctx: &Context,
    transform: tiny_skia::Transform,
    pixmap: &mut tiny_skia::Pixmap,
    cache: &mut crate::cache::SvgrCache,
) {
    if mask.root().children().is_empty() {
        pixmap.fill(tiny_skia::Color::TRANSPARENT);
        return;
    }

    let mask_pixmap = cache
        .with_subpixmap_cache(
            mask,
            IntSize::from_wh(pixmap.width(), pixmap.height()).unwrap(),
            |mask_pixmap, cache| {
                // TODO: only when needed
                // Mask has to be clipped by mask.region
                let mut alpha_mask = tiny_skia::Mask::new(pixmap.width(), pixmap.height()).unwrap();
                alpha_mask.fill_path(
                    &tiny_skia::PathBuilder::from_rect(mask.rect().to_rect()),
                    tiny_skia::FillRule::Winding,
                    true,
                    transform,
                );

                crate::render::render_nodes(
                    mask.root(),
                    ctx,
                    transform,
                    &mut mask_pixmap.as_mut(),
                    cache,
                );

                mask_pixmap.apply_mask(&alpha_mask);

                Some(())
            },
        )
        .expect("failed to allocate pixmap for mask");

    if let Some(mask) = mask.mask() {
        self::apply(mask, ctx, transform, pixmap, &mut SvgrCache::none());
    }

    let mask_type = match mask.kind() {
        usvgr::MaskType::Luminance => tiny_skia::MaskType::Luminance,
        usvgr::MaskType::Alpha => tiny_skia::MaskType::Alpha,
    };

    let mask = tiny_skia::Mask::from_pixmap(mask_pixmap.as_ref(), mask_type);
    pixmap.apply_mask(&mask);
}
