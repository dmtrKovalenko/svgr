// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use crate::{cache, render::Context, PixmapPool, SvgrCache};
use tiny_skia::IntSize;

pub fn apply(
    clip: &usvgr::ClipPath,
    transform: tiny_skia::Transform,
    pixmap: &mut tiny_skia::Pixmap,
    cache: &mut cache::SvgrCache,
    pixmap_pool: &cache::PixmapPool,
) {
    let clip_pixmap = cache
        .with_subpixmap_cache(
            clip,
            transform,
            pixmap_pool,
            IntSize::from_wh(pixmap.width(), pixmap.height()).unwrap(),
            |mut clip_pixmap, cache| {
                clip_pixmap.fill(tiny_skia::Color::BLACK);

                draw_children(
                    clip.root(),
                    tiny_skia::BlendMode::Clear,
                    transform.pre_concat(clip.transform()),
                    &mut clip_pixmap.as_mut(),
                    cache,
                    &PixmapPool::new(),
                );

                Some(clip_pixmap)
            },
        )
        .expect("failed to allocate pixmap for clip");

    if let Some(clip) = clip.clip_path() {
        // here we are handling the recurision on self, and while we hold the reference to the
        // cache lru instance this will OVERWRITE the existing pixmpa or cache entry
        apply(
            clip,
            transform,
            pixmap,
            &mut SvgrCache::none(),
            &PixmapPool::new(),
        );
    }

    let mut mask = tiny_skia::Mask::from_pixmap(clip_pixmap.as_ref(), tiny_skia::MaskType::Alpha);
    mask.invert();
    pixmap.apply_mask(&mask);
}

fn draw_children(
    parent: &usvgr::Group,
    mode: tiny_skia::BlendMode,
    transform: tiny_skia::Transform,
    pixmap: &mut tiny_skia::PixmapMut,
    cache: &mut cache::SvgrCache,
    pixmap_pool: &cache::PixmapPool,
) {
    for child in parent.children() {
        match child {
            usvgr::Node::Path(ref path) => {
                if path.visibility() != usvgr::Visibility::Visible {
                    continue;
                }

                let ctx = Context {
                    // We could use any values here. They will not be used anyway.
                    max_bbox: tiny_skia::IntRect::from_xywh(0, 0, 1, 1).unwrap(),
                };

                crate::path::fill_path(path, mode, &ctx, transform, pixmap, cache, pixmap_pool);
            }
            usvgr::Node::Text(ref text) => {
                draw_children(
                    text.flattened(),
                    mode,
                    transform,
                    pixmap,
                    cache,
                    pixmap_pool,
                );
            }
            usvgr::Node::Group(ref group) => {
                let transform = transform.pre_concat(group.transform());

                if let Some(clip) = group.clip_path() {
                    // If a `clipPath` child also has a `clip-path`
                    // then we should render this child on a new canvas,
                    // clip it, and only then draw it to the `clipPath`.
                    clip_group(group, clip, transform, pixmap, cache, pixmap_pool);
                } else {
                    draw_children(group, mode, transform, pixmap, cache, pixmap_pool);
                }
            }
            _ => {}
        }
    }
}

fn clip_group(
    children: &usvgr::Group,
    clip: &usvgr::ClipPath,
    transform: tiny_skia::Transform,
    pixmap: &mut tiny_skia::PixmapMut,
    cache: &mut cache::SvgrCache,
    pixmap_pool: &cache::PixmapPool,
) -> Option<()> {
    let mut clip_pixmap = pixmap_pool
        .take_or_allocate(pixmap.width(), pixmap.height())
        .unwrap();

    draw_children(
        children,
        tiny_skia::BlendMode::SourceOver,
        transform,
        &mut clip_pixmap.as_mut(),
        cache,
        pixmap_pool,
    );
    apply(clip, transform, &mut clip_pixmap, cache, pixmap_pool);

    let mut paint = tiny_skia::PixmapPaint::default();
    paint.blend_mode = tiny_skia::BlendMode::Xor;
    pixmap.draw_pixmap(
        0,
        0,
        clip_pixmap.as_ref(),
        &paint,
        tiny_skia::Transform::identity(),
        None,
    );

    Some(())
}
