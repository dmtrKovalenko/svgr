use lru::LruCache;
use std::cell::{RefCell, RefMut};
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
}

impl SvgrCache {
    /// Creates a new cache with the specified capacity.
    /// If capacity <= 0 then cache is disabled and this struct does not allocate.
    /// Uses `ahash` as a hasher, if you want to specify custom hasher user `new_with_hasher` fn.
    pub fn new(size: usize) -> Self {
        Self::new_sized(size)
    }
} 

/// Pixmap ring buffer required for reusing allocation when creating a lot of inner pixmaps
#[derive(Debug)]
pub struct PixmapPool<HashBuilder: BuildHasher = ahash::RandomState> {
    // Store pixmaps by size for efficient reuse
    buffers: RefCell<HashMap<(u32, u32), Pixmap, HashBuilder>>,
}

impl<THashBuilder: BuildHasher + Default> PixmapPool<THashBuilder> {
    /// Create new unbounded pixmap pool
    pub fn new() -> Self {
        Self {
            buffers: RefCell::new(HashMap::with_hasher(THashBuilder::default())),
        }
    }

    #[allow(unused)]
    pub(crate) fn get_or_allocate(&self, width: u32, height: u32) -> Option<RefMut<Pixmap>> {
        let mut buffers = self.buffers.borrow_mut();

        match buffers.entry((width, height)) {
            Entry::Occupied(pixmap) => {
                let existing = pixmap.into_mut();
                existing.data_mut().fill(0);

                // Drop the borrow_mut before creating a new one
                drop(buffers);

                // Create a reference to the specific pixmap
                Some(RefMut::map(self.buffers.borrow_mut(), |map| {
                    map.get_mut(&(width, height)).unwrap()
                }))
            }
            Entry::Vacant(vacant_entry) => {
                let new_pixmap = Pixmap::new(width, height).log_none(|| {
                    log::warn!("Failed to allocate a group layer for sub pixmap for hashing")
                })?;

                vacant_entry.insert(new_pixmap);

                // Drop the borrow_mut before creating a new one
                drop(buffers);

                // Create a reference to the specific pixmap
                Some(RefMut::map(self.buffers.borrow_mut(), |map| {
                    map.get_mut(&(width, height)).unwrap()
                }))
            }
        }
    }

    pub(crate) fn take_or_allocate(&self, width: u32, height: u32) -> Option<Pixmap> {
        let mut buffers = self.buffers.borrow_mut();

        match buffers.entry((width, height)) {
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

    pub(crate) fn release<'a>(&'a self, pixmap: Pixmap) -> &'a Pixmap {
        let size_key = (pixmap.width(), pixmap.height());
        let mut buffers = self.buffers.borrow_mut();

        // Insert the pixmap and get a reference to it
        let entry = buffers.entry(size_key).insert_entry(pixmap);
        let owned_pixmap = entry.get();

        // We need to extend the lifetime of the pixmap becuase we don't want to be bound to the 
        // refcell this is safe because we always know that pixamp will be present in the buffer
        // no matter what, the only potential issue might be realted to the recursive writes 
        // (if during the lifetime of this reference someone will write something)
        unsafe {
            std::mem::transmute::<&Pixmap, &'a Pixmap>(owned_pixmap)
        }
    }
}

impl<THashBuilder: BuildHasher + Default> SvgrCache<THashBuilder> {
    /// Creates a no cache value. Basically an Option::None.
    pub fn none() -> Self {
        Self { cache: None }
    }

    /// Creates a new cache with the specified capacity.
    /// If capacity <= 0 then cache is disabled.
    pub fn new_sized(size: usize) -> Self {
        if size > 0 {
            Self {
                cache: Some(SvgrCacheInternal {
                    lru: LruCache::new(std::num::NonZeroUsize::new(size).unwrap()),
                    hash_builder: THashBuilder::default(),
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
    pub(crate) fn with_subpixmap_cache<'a, F: FnOnce(Pixmap, &mut Self) -> Option<Pixmap>>(
        &'a mut self,
        node: &impl Hash,
        pixmap_pool: &'a PixmapPool,
        size: IntSize,
        f: F,
    ) -> Option<&'a Pixmap> {
        if self.cache.is_none() {
            let pixmap = pixmap_pool.take_or_allocate(size.width(), size.height())?;
            let pixmap = { f(pixmap, &mut Self::none()) }?;
            let value = pixmap_pool.release(pixmap);
            return Some(value);
        }

        let hash = self.hash(node)?;

        if !self.lru()?.contains(&hash) {
            let pixmap = pixmap_pool.take_or_allocate(size.width(), size.height())?;

            let pixmap = { f(pixmap, self) }?;

            // we basically passing down the mutable ref and getting it back
            // this is a primitive way to achieve recurisve mutable borrowing
            // without any overhead of Rc or RefCell
            if let Some((_, cache_back)) = self.lru()?.push(hash, pixmap) {
                pixmap_pool.release(cache_back);
            }
        }

        let pixmap = self.lru()?.peek(&hash)?;
        return Some(pixmap);
    }
}
