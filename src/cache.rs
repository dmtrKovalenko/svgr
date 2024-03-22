use std::hash::{BuildHasher, Hash, Hasher};

use crate::{render::Canvas, trim_transparency};
use lru::LruCache;
use tiny_skia::PixmapPaint;
use usvgr::HashedNode;

struct SvgrCacheInternal<HashBuilder: BuildHasher = ahash::RandomState> {
    lru: LruCache<u64, FromPixmap>,
    hash_builder: HashBuilder,
}

/// Defines rendering LRU cache. Each individual node and group will be cached separately.
/// Make sure that in most cases it will require saving of the whole canvas which may lead to significant memory usage.
/// So it is recommended to set the cache size to a reasonable value.
///
/// Pass &mut SvgrCache::none() if you don't need caching.
pub struct SvgrCache<RandomState: BuildHasher = ahash::RandomState>(
    Option<SvgrCacheInternal<RandomState>>,
);

pub struct FromPixmap {
    pub pixmap: tiny_skia::Pixmap,
    pub blend_mode: tiny_skia::BlendMode,
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
                blend_mode: self.blend_mode,
                quality: tiny_skia::FilterQuality::Nearest,
            },
            tiny_skia::Transform::default(),
            None,
        );
    }
}

impl SvgrCache {
    /// Creates a new cache with the specified capacity.
    /// If capacity <= 0 then cache is disabled and this struct does not allocate.
    /// Uses `ahash` as a hasher, if you want to specify custom hasher user `new_with_hasher` fn.
    pub fn new(size: usize) -> Self {
        Self::new_with_hasher(size, ahash::RandomState::default())
    }
}

impl<THashBuilder: BuildHasher + Default> SvgrCache<THashBuilder> {
    /// Creates a no cache value. Basically an Option::None.
    pub fn none() -> Self {
        Self(None)
    }

    /// Creates a new cache with the specified capacity.
    /// If capacity <= 0 then cache is disabled.
    pub fn new_with_hasher(size: usize, hasher: THashBuilder) -> Self {
        if size > 0 {
            Self(Some(SvgrCacheInternal {
                lru: LruCache::new(std::num::NonZeroUsize::new(size).unwrap()),
                hash_builder: hasher,
            }))
        } else {
            Self::empty()
        }
    }

    /// Creates disabled cache object
    pub fn empty() -> Self {
        Self(None)
    }

    fn hash(&self, node: &usvgr::Node) -> Option<u64> {
        let cache = self.0.as_ref()?;

        let mut hasher = cache.hash_builder.build_hasher();
        HashedNode(node).hash(&mut hasher);
        Some(Hasher::finish(&hasher))
    }

    /// Creates sub pixmap that will be cached itself within a canvas cache. Guarantees empty canvas within closure.  
    pub(crate) fn with_subpixmap_cache(
        &mut self,
        node: &usvgr::Node,
        canvas: &mut Canvas,
        mut f: impl FnMut(&mut Canvas, &mut SvgrCache<THashBuilder>) -> FromPixmap,
    ) {
        let hash = self.hash(node);
        let cached_value = self
            .0
            .as_mut()
            .zip(hash)
            .and_then(|(cache, hash)| cache.lru.get(&hash));

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

            if let Some((cache, hash)) = self.0.as_mut().zip(hash) {
                cache.lru.put(hash, value);
            }
        };
    }

    pub(crate) fn with_cache(
        &mut self,
        canvas: &mut Canvas,
        node: &usvgr::Node,
        mut f: impl FnMut(&mut Canvas, &mut SvgrCache<THashBuilder>),
    ) {
        if canvas.skip_caching {
            f(canvas, self);
            return;
        }

        let hash = self.hash(node);
        let cached_value = self
            .0
            .as_mut()
            .zip(hash)
            .and_then(|(cache, hash)| cache.lru.get(&hash));

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
                    blend_mode: tiny_skia::BlendMode::SourceOver,
                }
            } else {
                FromPixmap {
                    pixmap,
                    tx: 0,
                    ty: 0,
                    opacity: 1.0,
                    blend_mode: tiny_skia::BlendMode::SourceOver,
                }
            };

            value.draw_into(canvas);

            if let Some((cache, hash)) = self.0.as_mut().zip(hash) {
                cache.lru.put(hash, value);
            }
        };
    }
}
