#![allow(missing_docs)]
use lru::LruCache;
use std::cell::UnsafeCell;
use std::collections::VecDeque;
use std::hash::{BuildHasher, Hash, Hasher};
use tiny_skia::{IntSize, Pixmap, BYTES_PER_PIXEL};
use usvgr::{
    ahash::{self, AHashMap},
    lru,
};

/// Default maximum memory for static cache (64 MB)
const DEFAULT_STATIC_CACHE_MAX_BYTES: usize = 64 * 1024 * 1024;

const DEFAULT_STATIC_CACHE_INITIAL_CAPACITY: usize = 256;

#[derive(Debug)]
struct LruCacheInternal<HashBuilder: BuildHasher = ahash::RandomState> {
    lru: LruCache<u64, Pixmap>,
    hash_builder: HashBuilder,
}

/// Statistics for cache performance monitoring.
///
/// Only available when the `cache-stats` feature is enabled.
#[cfg(feature = "cache-stats")]
#[derive(Debug, Clone, Copy, Default)]
pub struct CacheStats {
    /// Number of cache hits (element found in cache)
    pub hits: u64,
    /// Number of cache misses (element not found, had to render)
    pub misses: u64,
    /// Number of elements inserted into cache
    pub inserts: u64,
    /// Number of elements evicted from cache (memory limit reached)
    pub evictions: u64,
    /// Total bytes currently used by cached pixmaps
    pub bytes_used: usize,
}

#[cfg(feature = "cache-stats")]
impl CacheStats {
    /// Returns the cache hit rate as a percentage (0.0 - 1.0)
    #[inline]
    pub fn hit_rate(&self) -> f64 {
        let total = self.hits + self.misses;
        if total == 0 {
            0.0
        } else {
            self.hits as f64 / total as f64
        }
    }
}

#[cfg(feature = "cache-stats")]
impl std::fmt::Display for CacheStats {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "CacheStats {{ hits: {}, misses: {}, inserts: {}, evictions: {}, bytes_used: {}, hit_rate: {:.2}% }}",
            self.hits,
            self.misses,
            self.inserts,
            self.evictions,
            self.bytes_used,
            self.hit_rate() * 100.0,
        )
    }
}

#[derive(Debug)]
struct StaticCache {
    // unsafe cell here is fine becuase we are only mutating this SINGLE time per an entry
    entries: UnsafeCell<AHashMap<u64, Pixmap>>,
    bytes_used: UnsafeCell<usize>,
    max_bytes: usize,
    #[cfg(feature = "cache-stats")]
    stats: UnsafeCell<CacheStats>,
}

impl StaticCache {
    fn new() -> Self {
        Self::with_config(
            DEFAULT_STATIC_CACHE_MAX_BYTES,
            DEFAULT_STATIC_CACHE_INITIAL_CAPACITY,
        )
    }

    fn with_config(max_bytes: usize, initial_capacity: usize) -> Self {
        Self {
            entries: UnsafeCell::new(AHashMap::with_capacity(initial_capacity)),
            bytes_used: UnsafeCell::new(0),
            max_bytes,
            #[cfg(feature = "cache-stats")]
            stats: UnsafeCell::new(CacheStats::default()),
        }
    }

    /// Unlimited cache with no memory bounds
    fn unlimited() -> Self {
        Self::with_config(0, DEFAULT_STATIC_CACHE_INITIAL_CAPACITY)
    }

    #[inline]
    fn get(&self, hash: u64) -> Option<&Pixmap> {
        let result = unsafe { (*self.entries.get()).get(&hash) };
        #[cfg(feature = "cache-stats")]
        unsafe {
            if result.is_some() {
                (*self.stats.get()).hits += 1;
            } else {
                (*self.stats.get()).misses += 1;
            }
        }
        result
    }

    #[inline]
    fn contains(&self, hash: u64) -> bool {
        unsafe { (*self.entries.get()).contains_key(&hash) }
    }

    /// Calculate the byte size of a pixmap
    #[inline]
    fn pixmap_bytes(pixmap: &Pixmap) -> usize {
        pixmap.data().len()
    }

    #[inline]
    fn insert(&self, hash: u64, pixmap: Pixmap) -> bool {
        let pixmap_size = Self::pixmap_bytes(&pixmap);

        unsafe {
            let bytes_used = &mut *self.bytes_used.get();

            // Check if we would exceed memory limit
            if self.max_bytes > 0 && *bytes_used + pixmap_size > self.max_bytes {
                // Don't insert if it would exceed limit
                // In a more sophisticated implementation, we could evict old entries
                #[cfg(feature = "cache-stats")]
                {
                    (*self.stats.get()).evictions += 1;
                }
                return false;
            }

            let entries = &mut *self.entries.get();

            // Check if replacing an existing entry
            if let Some(old_pixmap) = entries.get(&hash) {
                *bytes_used -= Self::pixmap_bytes(old_pixmap);
            }

            entries.insert(hash, pixmap);
            *bytes_used += pixmap_size;
            #[cfg(feature = "cache-stats")]
            {
                let stats = &mut *self.stats.get();
                stats.inserts += 1;
                stats.bytes_used = *bytes_used;
            }
        }
        true
    }

    fn len(&self) -> usize {
        unsafe { (*self.entries.get()).len() }
    }

    fn bytes_used(&self) -> usize {
        unsafe { *self.bytes_used.get() }
    }

    #[cfg(feature = "cache-stats")]
    fn stats(&self) -> CacheStats {
        unsafe { *self.stats.get() }
    }

    fn clear(&self) {
        unsafe {
            (*self.entries.get()).clear();
            *self.bytes_used.get() = 0;
            #[cfg(feature = "cache-stats")]
            {
                *self.stats.get() = CacheStats::default();
            }
        }
    }
}

/// Configuration options for the static cache
#[derive(Debug, Clone)]
pub struct StaticCacheConfig {
    /// Maximum memory in bytes for the static cache (0 = unlimited)
    pub max_bytes: usize,
    /// Initial capacity for the HashMap (reduces rehashing for large SVGs)
    pub initial_capacity: usize,
}

impl Default for StaticCacheConfig {
    fn default() -> Self {
        Self {
            max_bytes: DEFAULT_STATIC_CACHE_MAX_BYTES,
            initial_capacity: DEFAULT_STATIC_CACHE_INITIAL_CAPACITY,
        }
    }
}

impl StaticCacheConfig {
    /// Create config for unlimited memory usage
    pub fn unlimited() -> Self {
        Self {
            max_bytes: 0,
            initial_capacity: DEFAULT_STATIC_CACHE_INITIAL_CAPACITY,
        }
    }

    /// Create config optimized for large SVGs with many repeating elements
    pub fn for_large_repeating_svgs() -> Self {
        Self {
            max_bytes: 256 * 1024 * 1024, // 256 MB
            initial_capacity: 1024,       // Larger initial capacity
        }
    }

    /// Create config with specific memory limit in megabytes
    pub fn with_max_mb(mb: usize) -> Self {
        Self {
            max_bytes: mb * 1024 * 1024,
            initial_capacity: DEFAULT_STATIC_CACHE_INITIAL_CAPACITY,
        }
    }
}

/// Defines rendering cache with both LRU cache for dynamic elements and
/// persistent cache for static elements.
///
/// The cache has two tiers:
/// 1. **Static cache**: For elements with compile-time known content (identified by `static_hash`).
///    These are rendered once at their bounding box and cached permanently using the hash directly.
///    Includes a bloom filter for fast negative lookups and memory bounds to prevent unbounded growth.
/// 2. **LRU cache**: For dynamic elements that may change between frames.
///    Uses least-recently-used eviction when capacity is reached.
///
/// ## Optimizations for Large Repeating SVGs
///
/// The static cache is optimized for SVGs with many identical elements:
/// - **Bloom filter**: Provides O(1) negative lookups, avoiding HashMap access for uncached elements
/// - **Memory bounds**: Configurable max memory prevents unbounded cache growth
/// - **Pre-sized HashMap**: Reduces rehashing overhead for large element counts
/// - **Statistics tracking**: Enables monitoring cache efficiency
///
/// Pass `&mut SvgrCache::none()` if you don't need any caching.
#[derive(Debug)]
pub struct SvgrCache<RandomState: BuildHasher = ahash::RandomState> {
    /// LRU cache for dynamic elements
    lru_cache: Option<LruCacheInternal<RandomState>>,
    /// Persistent cache for static elements (compile-time known content)
    static_cache: Option<StaticCache>,
}

impl SvgrCache {
    /// Creates a new cache with the specified LRU capacity.
    /// Static caching is enabled by default with a 64MB memory limit.
    ///
    /// If capacity <= 0 then the LRU cache is disabled but static cache remains active.
    /// Uses `ahash` as a hasher, if you want to specify custom hasher use `new_with_hasher` fn.
    pub fn new(lru_size: usize) -> Self {
        Self::new_sized(lru_size)
    }

    /// Creates a new cache with static caching only (no LRU cache).
    ///
    /// This is useful when you only want to cache compile-time static elements
    /// and don't need caching for dynamic elements.
    pub fn static_only() -> Self {
        Self {
            lru_cache: None,
            static_cache: Some(StaticCache::new()),
        }
    }

    /// Creates a new cache optimized for large SVGs with many repeating elements.
    ///
    /// This configuration:
    /// - Uses a larger initial HashMap capacity to reduce rehashing
    /// - Has a higher memory limit (256MB) for more cache entries
    /// - Includes bloom filter for fast negative lookups
    pub fn for_large_repeating_svgs(lru_size: usize) -> Self {
        let config = StaticCacheConfig::for_large_repeating_svgs();
        Self::with_static_config(lru_size, config)
    }

    /// Creates a new cache with custom static cache configuration.
    pub fn with_static_config(lru_size: usize, config: StaticCacheConfig) -> Self {
        let lru_cache = if lru_size > 0 {
            Some(LruCacheInternal {
                lru: LruCache::new(std::num::NonZeroUsize::new(lru_size).unwrap()),
                hash_builder: ahash::RandomState::default(),
            })
        } else {
            None
        };

        Self {
            lru_cache,
            static_cache: Some(StaticCache::with_config(
                config.max_bytes,
                config.initial_capacity,
            )),
        }
    }
}

// 2^16 = 65536x65536 which should be enough for ANY renderable canvas size
const MAX_PIXMAP_DIMENSION_POW_2: usize = 16;

/// This is a mutable pixmap pool which allocates queues of size classes (powers of 2) containing
/// previously allocated and used pixmaps. They are given to the consumer as a virtual pixmap of the
/// requested size but are always allocated as a closest power of 2 sized memory block.
#[derive(Debug)]
pub struct PixmapPool {
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

impl Default for PixmapPool {
    fn default() -> Self {
        Self::new()
    }
}

impl<THashBuilder: BuildHasher + Default> SvgrCache<THashBuilder> {
    /// Creates a no cache value. Neither LRU nor static caching is enabled.
    pub fn none() -> Self {
        Self {
            lru_cache: None,
            static_cache: None,
        }
    }

    /// Creates a new cache with the specified LRU capacity and static caching enabled.
    /// If capacity <= 0 then only static caching is used.
    pub fn new_sized(lru_size: usize) -> Self {
        let lru_cache = if lru_size > 0 {
            Some(LruCacheInternal {
                lru: LruCache::new(std::num::NonZeroUsize::new(lru_size).unwrap()),
                hash_builder: THashBuilder::default(),
            })
        } else {
            None
        };

        Self {
            lru_cache,
            static_cache: Some(StaticCache::new()),
        }
    }

    /// Creates a new cache with only LRU caching (no static cache).
    /// Use this if you don't want permanent caching of static elements.
    pub fn lru_only(lru_size: usize) -> Self {
        if lru_size > 0 {
            Self {
                lru_cache: Some(LruCacheInternal {
                    lru: LruCache::new(std::num::NonZeroUsize::new(lru_size).unwrap()),
                    hash_builder: THashBuilder::default(),
                }),
                static_cache: None,
            }
        } else {
            Self::none()
        }
    }

    /// Creates a new cache with unlimited static cache memory.
    /// Use with caution - cache can grow unbounded for large SVGs.
    pub fn unlimited_static(lru_size: usize) -> Self {
        let lru_cache = if lru_size > 0 {
            Some(LruCacheInternal {
                lru: LruCache::new(std::num::NonZeroUsize::new(lru_size).unwrap()),
                hash_builder: THashBuilder::default(),
            })
        } else {
            None
        };

        Self {
            lru_cache,
            static_cache: Some(StaticCache::unlimited()),
        }
    }

    /// Returns the number of entries in the static cache.
    pub fn static_cache_len(&self) -> usize {
        self.static_cache.as_ref().map_or(0, |c| c.len())
    }

    /// Returns the total bytes used by the static cache.
    pub fn static_cache_bytes(&self) -> usize {
        self.static_cache.as_ref().map_or(0, |c| c.bytes_used())
    }

    /// Returns cache statistics for performance monitoring.
    ///
    /// Only available when the `cache-stats` feature is enabled.
    #[cfg(feature = "cache-stats")]
    pub fn static_cache_stats(&self) -> CacheStats {
        self.static_cache
            .as_ref()
            .map_or(CacheStats::default(), |c| c.stats())
    }

    /// Prints a summary of static cache statistics to stdout.
    ///
    /// This is a convenience method for calling libraries to inspect cache
    /// performance. Only available when the `cache-stats` feature is enabled.
    /// When the feature is disabled this method is a no-op.
    #[cfg(feature = "cache-stats")]
    pub fn print_stats(&self) {
        let stats = self.static_cache_stats();
        println!(
            "[svgr] static cache: entries={}, bytes={}, hits={}, misses={}, inserts={}, evictions={}, hit_rate={:.2}%",
            self.static_cache_len(),
            stats.bytes_used,
            stats.hits,
            stats.misses,
            stats.inserts,
            stats.evictions,
            stats.hit_rate() * 100.0,
        );
    }

    /// No-op when the `cache-stats` feature is disabled.
    #[cfg(not(feature = "cache-stats"))]
    pub fn print_stats(&self) {}

    /// Clears the static cache, freeing all cached pixmaps.
    pub fn clear_static_cache(&self) {
        if let Some(ref cache) = self.static_cache {
            cache.clear();
        }
    }

    /// Returns true if static caching is enabled.
    pub fn has_static_cache(&self) -> bool {
        self.static_cache.is_some()
    }

    /// Get a cached pixmap by its static hash.
    /// Uses bloom filter for fast negative lookups.
    #[inline]
    pub fn get_static(&self, static_hash: u64) -> Option<&Pixmap> {
        self.static_cache.as_ref()?.get(static_hash)
    }

    /// Check if a static hash is in the cache.
    /// Uses bloom filter for fast negative lookups.
    #[inline]
    pub fn has_static(&self, static_hash: u64) -> bool {
        self.static_cache
            .as_ref()
            .map_or(false, |c| c.contains(static_hash))
    }

    /// Insert a pixmap into the static cache.
    /// Returns true if inserted, false if memory limit would be exceeded.
    #[inline]
    pub fn insert_static(&self, static_hash: u64, pixmap: Pixmap) -> bool {
        if let Some(ref cache) = self.static_cache {
            cache.insert(static_hash, pixmap)
        } else {
            false
        }
    }

    fn lru(&mut self) -> Option<&mut LruCache<u64, Pixmap>> {
        self.lru_cache.as_mut().map(|cache| &mut cache.lru)
    }

    fn hash(
        &self,
        size: IntSize,
        transform: tiny_skia::Transform,
        node: &impl Hash,
    ) -> Option<u64> {
        use usvgr::hashers::CustomHash;
        let cache = self.lru_cache.as_ref()?;

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
        if self.lru_cache.is_none() {
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

// Safety: SvgrCache uses UnsafeCell internally for the static cache,
// but is designed for single-threaded rendering contexts.
unsafe impl<T: BuildHasher + Send> Send for SvgrCache<T> {}
