use lru::LruCache;
use std::cell::UnsafeCell;
use std::collections::VecDeque;
use std::hash::{BuildHasher, Hash, Hasher};
use tiny_skia::{IntSize, Pixmap, BYTES_PER_PIXEL};
use usvgr::{
    ahash::{self},
    lru,
};

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

// 2^16 = 65536x65536 which should be enough for ANY renderable canvas size
const MAX_PIXMAP_DIMENSION_POW_2: usize = 16;

/// This is a pixmap pool that preallocates pixmaps of various (power of 2s) sizes
/// and then reuses them without reallocating memory for all the requested pixel sizes
/// that are larger or equal to the `SIZE^2xSIZE^2` size
#[derive(Debug)]
pub struct PixmapPool {
    /// This is a mutable staack allocated array of size classes (powers of 2) which contains a
    /// vector of pixmap already which are given to the consumer as a virtual pixmap of the
    /// requested size but are always allocated as a closest power of 2 sized memory block.
    ///
    /// We guarantee that the pixmap pool is leaving longer than the memory but wrapping this in a
    /// life time is a way to much work for the fork, so sticking to the no runtime check ref
    /// instead.
    size_classes: UnsafeCell<[VecDeque<Pixmap>; MAX_PIXMAP_DIMENSION_POW_2]>,
}

impl PixmapPool {
    /// Creates a new pixmap pool without any preallocated pixmaps.
    pub fn new() -> Self {
        Self {
            size_classes: UnsafeCell::new(std::array::from_fn(|_| VecDeque::new())),
        }
    }

    /// Creates a new pixmap pool with the specified capacity
    pub fn new_with_capacity(capacity: usize) -> Self {
        let size_classes = std::array::from_fn(|i| {
            if i < 8 {
                VecDeque::with_capacity(capacity)
            } else {
                VecDeque::with_capacity(capacity / 2)
            }
        });

        Self {
            size_classes: UnsafeCell::new(size_classes),
        }
    }

    fn next_power_of_2(n: u32) -> u32 {
        1 << (32 - (n - 1).leading_zeros())
    }

    fn standard_square_size(width: u32, height: u32) -> u32 {
        let max_dim = u32::max(width, height);
        Self::next_power_of_2(max_dim)
    }

    fn safe_size_class_index(width: u32) -> usize {
        let size_class = width.trailing_zeros() as usize;
        if size_class >= MAX_PIXMAP_DIMENSION_POW_2 {
            panic!(
                "Can not render pixmap with a size larger than 2^{}",
                MAX_PIXMAP_DIMENSION_POW_2
            );
        }

        size_class
    }

    fn data_len_for_size(size: IntSize) -> Option<usize> {
        let length = size.width().checked_mul(size.height())? as usize;

        length.checked_mul(BYTES_PER_PIXEL)
    }

    pub(crate) fn take_or_allocate(&self, width: u32, height: u32) -> Option<Pixmap> {
        let size = Self::standard_square_size(width, height);
        let class_index = Self::safe_size_class_index(size);
        let virtual_size = IntSize::from_wh(width, height)?;
        let virtual_data_len = Self::data_len_for_size(virtual_size)?;

        let mut buffer = unsafe {
            let size_classes = &mut *self.size_classes.get();
            size_classes[class_index]
                .pop_back()
                .map(|pixmap| pixmap.take())
        }
        .or_else(|| {
            let std_size = IntSize::from_wh(size, size)?;
            let std_data_len = Self::data_len_for_size(std_size)?;
            Some(vec![0; std_data_len])
        })?;

        unsafe {
            buffer.set_len(virtual_data_len);
        }
        buffer.fill(0);

        Pixmap::from_vec(buffer, virtual_size)
    }

    pub(crate) fn release<'a>(&'a self, pixmap: Pixmap) -> &'a Pixmap {
        let virtual_width = pixmap.width();
        let virtual_height = pixmap.height();
        let size = Self::standard_square_size(virtual_width, virtual_height);
        let class_index = Self::safe_size_class_index(size);

        unsafe {
            let size_classes = &mut *self.size_classes.get();
            size_classes[class_index].push_back(pixmap);
            // Safe because we just pushed the pixmap and the pool is guaranteed to live longer
            size_classes[class_index]
                .back()
                .expect("Failed to get back stored pixmap")
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

    fn hash(
        &self,
        size: IntSize,
        transform: tiny_skia::Transform,
        node: &impl Hash,
    ) -> Option<u64> {
        use usvgr::hashers::CustomHash;
        let cache = self.cache.as_ref()?;

        let mut hasher = cache.hash_builder.build_hasher();
        node.hash(&mut hasher);
        size.width().hash(&mut hasher);
        size.height().hash(&mut hasher);
        transform.custom_hash(&mut hasher);
        Some(Hasher::finish(&hasher))
    }

    pub(crate) fn with_subpixmap_cache<'a, F: FnOnce(Pixmap, &mut Self) -> Option<Pixmap>>(
        &'a mut self,
        node: &impl Hash,
        transform: tiny_skia::Transform,
        pixmap_pool: &'a PixmapPool,
        size: IntSize,
        f: F,
    ) -> Option<&'a Pixmap> {
        if self.cache.is_none() {
            let pixmap = pixmap_pool.take_or_allocate(size.width(), size.height())?;
            let pixmap = { f(pixmap, self) }?;
            let value = pixmap_pool.release(pixmap);
            return Some(value);
        }

        let hash = self.hash(size, transform, node)?;

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
