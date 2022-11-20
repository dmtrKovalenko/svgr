use std::{
    collections::hash_map::DefaultHasher,
    hash::{Hash, Hasher},
};

use crate::{render::Canvas, trim_transparency};
use lru::LruCache;
use tiny_skia::PixmapPaint;
use usvgr::HashedNode;

/// Defines rendering LRU cache. Each individual node and group will be cached separately.
/// Make sure that in most cases it will require saving of the whole canvas which may lead to significant memory usage.
/// So it is recommended to set the cache size to a reasonable value.
///
/// Pass &mut SvgrCache::none() if you don't need caching.
pub struct SvgrCache(Option<LruCache<u64, FromPixmap>>);

pub struct FromPixmap {
    pub pixmap: tiny_skia::Pixmap,
    pub tx: i32,
    pub ty: i32,
    pub opacity: f32,
}

impl FromPixmap {
    fn draw_into(&self, canvas: &mut Canvas) {
        canvas.pixmap.draw_pixmap(
            self.tx,
            self.ty,
            self.pixmap.as_ref(),
            &PixmapPaint {
                opacity: self.opacity,
                blend_mode: tiny_skia::BlendMode::SourceOver,
                quality: tiny_skia::FilterQuality::Nearest,
            },
            tiny_skia::Transform::default(),
            None,
        );
    }
}

impl SvgrCache {
    /// Creates a no cache value. Basically an Option::None.
    pub fn none() -> Self {
        Self(None)
    }

    /// Creates a new cache with the specified capacity.
    pub fn new(capacity: std::num::NonZeroUsize) -> Self {
        Self(Some(LruCache::new(capacity)))
    }

    fn hash(&self, node: &usvgr::Node) -> u64 {
        let mut hasher = DefaultHasher::new();
        HashedNode(node).hash(&mut hasher);
        hasher.finish()
    }

    /// Creates sub pixmap that will be cached itself withing a canvas cache. Guarantees empty canvas within closure.  
    pub(crate) fn with_subpixmap_cache(
        &mut self,
        node: &usvgr::Node,
        canvas: &mut Canvas,
        mut f: impl FnMut(&mut Canvas, &mut SvgrCache) -> FromPixmap,
    ) {
        let hash = self.hash(node);
        let cached_value = self.0.as_mut().and_then(|cache| cache.get(&hash));

        if let Some(cached_value) = cached_value {
            cached_value.draw_into(canvas)
        } else {
            let mut pixmap =
                tiny_skia::Pixmap::new(canvas.pixmap.width(), canvas.pixmap.height()).unwrap();
            let pixmap_mut = pixmap.as_mut();

            let mut temp_canvas = Canvas {
                pixmap: pixmap_mut,
                transform: canvas.transform,
                skip_caching: true,
                clip: canvas.clip.clone(),
            };

            let value = f(&mut temp_canvas, self);
            value.draw_into(canvas);

            if let Some(cache) = self.0.as_mut() {
                cache.put(hash, value);
            }
        };
    }

    pub(crate) fn with_cache(
        &mut self,
        canvas: &mut Canvas,
        node: &usvgr::Node,
        mut f: impl FnMut(&mut Canvas, &mut SvgrCache),
    ) {
        if canvas.skip_caching {
            f(canvas, self);
            return;
        }

        let hash = self.hash(node);
        let cached_value = self.0.as_mut().and_then(|cache| cache.get(&hash));

        if let Some(cached_value) = cached_value {
            cached_value.draw_into(canvas)
        } else {
            let mut pixmap =
                tiny_skia::Pixmap::new(canvas.pixmap.width(), canvas.pixmap.height()).unwrap();
            let pixmap_mut = pixmap.as_mut();

            let mut temp_canvas = Canvas {
                pixmap: pixmap_mut,
                transform: canvas.transform,
                skip_caching: true,
                clip: canvas.clip.clone(),
            };

            f(&mut temp_canvas, self);

            let value = if let Some((tx, ty, pixmap)) = trim_transparency(&mut pixmap.as_mut()) {
                FromPixmap {
                    pixmap,
                    tx,
                    ty,
                    opacity: 1.0,
                }
            } else {
                FromPixmap {
                    pixmap,
                    tx: 0,
                    ty: 0,
                    opacity: 1.0,
                }
            };

            value.draw_into(canvas);

            if let Some(cache) = self.0.as_mut() {
                cache.put(hash, value);
            }
        };
    }
}
