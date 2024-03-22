use std::env;

/// Here are the tests that verify that the caching works as expected, that there are no collisions
/// and no unexpected not updated nodes.
use crate::render_with_cache;
use svgr::SvgrCache;

#[test]
pub fn cache_stroke_dash_arraycache_stroke_dash_array() {
    let mut cache = SvgrCache::new(5);

    assert_eq!(render_with_cache("cache-dashoffset-001", &mut cache), 0);
    assert_eq!(render_with_cache("cache-dashoffset-002", &mut cache), 0);
}
