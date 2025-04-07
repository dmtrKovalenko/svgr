use lru::LruCache;
use std::collections::HashMap;
use std::{
    collections::hash_map::Entry,
    hash::{BuildHasher, Hash, Hasher},
};
use tiny_skia::{IntSize, Pixmap};
use usvgr::{
    ahash::{self},
    lru,
};

use crate::OptionLog;

#[derive(Debug)]
struct SvgrCacheInternal<HashBuilder: BuildHasher = ahash::RandomState> {
    lru: LruCache<u64, Pixmap>,
    hash_builder: HashBuilder,
}

/// Defines rendering LRU cache. Each individual node and group will be cached separately.
/// Make sure that in most cases it will require saving of the whole canvas which may lead to significant memory usage.
/// So it is recommended to set the cache size to a reasonable value.
///
/// Pass &mut SvgrCache::none() if you don't need caching.
#[derive(Debug)]
pub struct SvgrCache<RandomState: BuildHasher = ahash::RandomState> {
    cache: Option<SvgrCacheInternal<RandomState>>,
    pixmap_pool: PixmapPool<RandomState>,
}

impl SvgrCache {
    /// Creates a new cache with the specified capacity.
    /// If capacity <= 0 then cache is disabled and this struct does not allocate.
    /// Uses `ahash` as a hasher, if you want to specify custom hasher user `new_with_hasher` fn.
    pub fn new(size: usize) -> Self {
        Self::new_with_hasher(size, ahash::RandomState::default())
    }
}

#[derive(Debug)]
struct PixmapPool<HashBuilder: BuildHasher = ahash::RandomState> {
    // Store pixmaps by size for efficient reuse
    buffers: HashMap<(u32, u32), Pixmap, HashBuilder>,
}

impl<THashBuilder: BuildHasher + Default> PixmapPool<THashBuilder> {
    fn new() -> Self {
        Self {
            buffers: HashMap::with_hasher(THashBuilder::default()),
        }
    }

    fn get_or_allocate(&mut self, width: u32, height: u32) -> Option<&mut Pixmap> {
        match self.buffers.entry((width, height)) {
            Entry::Occupied(pixmap) => {
                let existing = pixmap.into_mut();
                existing.data_mut().fill(0);

                Some(existing)
            }
            Entry::Vacant(vacant_entry) => {
                let new_pixmap = Pixmap::new(width, height).log_none(|| {
                    log::warn!("Failed to allocate a group layer for sub pixmap for hashing")
                })?;

                Some(vacant_entry.insert(new_pixmap))
            }
        }
    }

    fn take_or_create_new(&mut self, width: u32, height: u32) -> Option<Pixmap> {
        match self.buffers.entry((width, height)) {
            Entry::Occupied(pixmap) => {
                let mut existing = pixmap.remove_entry().1;
                existing.data_mut().fill(0);
                Some(existing)
            }
            Entry::Vacant(_) => Pixmap::new(width, height).log_none(|| {
                log::warn!("Failed to allocate a group layer for sub pixmap for hashing")
            }),
        }
    }

    fn release(&mut self, pixmap: Pixmap) {
        let size_key = (pixmap.width(), pixmap.height());
        self.buffers.entry(size_key).or_insert(pixmap);
    }
}

impl<THashBuilder: BuildHasher + Default> SvgrCache<THashBuilder> {
    /// Creates a no cache value. Basically an Option::None.
    pub fn none() -> Self {
        Self {
            cache: None,
            pixmap_pool: PixmapPool::new(),
        }
    }

    /// Creates a new cache with the specified capacity.
    /// If capacity <= 0 then cache is disabled.
    pub fn new_with_hasher(size: usize, hasher: THashBuilder) -> Self {
        if size > 0 {
            Self {
                pixmap_pool: PixmapPool::new(),
                cache: Some(SvgrCacheInternal {
                    lru: LruCache::new(std::num::NonZeroUsize::new(size).unwrap()),
                    hash_builder: hasher,
                }),
            }
        } else {
            Self::none()
        }
    }

    fn lru(&mut self) -> Option<&mut LruCache<u64, Pixmap>> {
        self.cache.as_mut().map(|cache| &mut cache.lru)
    }

    fn hash(&self, node: &impl Hash) -> Option<u64> {
        let cache = self.cache.as_ref()?;

        let mut hasher = cache.hash_builder.build_hasher();
        node.hash(&mut hasher);
        Some(Hasher::finish(&hasher))
    }

    /// Creates sub pixmap that will be cached itself within a canvas cache. Guarantees empty canvas within closure.  
    pub(crate) fn with_subpixmap_cache<F: FnOnce(&mut Pixmap, &mut Self) -> Option<()>>(
        &mut self,
        node: &impl Hash,
        bbox: IntSize,
        f: F,
    ) -> Option<&Pixmap> {
        if self.cache.is_none() {
            let pixmap_ref = self
                .pixmap_pool
                .get_or_allocate(bbox.width(), bbox.height())?;

            f(pixmap_ref, &mut Self::none())?;
            return Some(pixmap_ref);
        }

        let hash = self.hash(node)?;
        let mut cache_ref = self;

        if !cache_ref.lru()?.contains(&hash) {
            let mut pixmap = cache_ref
                .pixmap_pool
                .take_or_create_new(bbox.width(), bbox.height())?;

            {
                f(&mut pixmap, &mut cache_ref)?
            };

            // we basically passing down the mutable ref and getting it back
            // this is a primitive way to achieve recurisve mutable borrowing
            // without any overhead of Rc or RefCell
            if let Some((_, cache_back)) = cache_ref.lru()?.push(hash, pixmap) {
                cache_ref.pixmap_pool.release(cache_back);
            }
        }

        let pixmap = cache_ref.lru()?.peek(&hash)?;
        return Some(pixmap);
    }
}
