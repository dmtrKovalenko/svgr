//! Benchmark for large SVGs with many repeating elements.
//!
//! This benchmark specifically tests cache efficiency when:
//! - Many identical elements are rendered (e.g., icon grids, tiled patterns)
//! - Elements share the same static_hash (compile-time identical content)
//! - Large numbers of elements stress memory and lookup performance
//!
//! Run with: cargo bench -p svgr --bench repeating_svg_benchmark

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use svgr::{render, Context, PixmapPool, StaticCacheConfig, SvgrCache};
use tiny_skia::Pixmap;
use usvgr::{fontdb, Options, Tree};

/// Creates an SVG with many identical repeating elements (simulates icon grids, tiled backgrounds)
fn create_repeating_svg(repeat_count: usize, unique_elements: usize) -> String {
    let mut defs = String::new();
    let mut uses = String::new();

    // Create unique symbol definitions
    for i in 0..unique_elements {
        let r = (i * 37) % 256;
        let g = (i * 59) % 256;
        let b = (i * 97) % 256;
        defs.push_str(&format!(
            concat!(
                r##"<symbol id="icon{}" viewBox="0 0 24 24">"##,
                r##"<rect x="2" y="2" width="20" height="20" rx="4" fill="rgb({},{},{})"/>"##,
                r##"<circle cx="12" cy="12" r="6" fill="white" opacity="0.5"/>"##,
                r##"<path d="M8,12 L16,12 M12,8 L12,16" stroke="white" stroke-width="2"/>"##,
                r##"</symbol>"##
            ),
            i, r, g, b
        ));
    }

    // Create repeated uses of the symbols
    let grid_cols = (repeat_count as f64).sqrt().ceil() as usize;
    for i in 0..repeat_count {
        let symbol_idx = i % unique_elements;
        let x = (i % grid_cols) * 30;
        let y = (i / grid_cols) * 30;
        uses.push_str(&format!(
            r##"<use href="#icon{}" x="{}" y="{}" width="24" height="24"/>"##,
            symbol_idx, x, y
        ));
    }

    let canvas_size = (grid_cols * 30 + 30).max(100);
    format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="{}" height="{}"><defs>{}</defs>{}</svg>"##,
        canvas_size, canvas_size, defs, uses
    )
}

/// Creates an SVG with many identical rectangles (simulates tiled backgrounds)
fn create_tiled_svg(tile_count: usize) -> String {
    let mut tiles = String::new();
    let grid_cols = (tile_count as f64).sqrt().ceil() as usize;

    for i in 0..tile_count {
        let x = (i % grid_cols) * 20;
        let y = (i / grid_cols) * 20;
        // All tiles are identical - should benefit heavily from caching
        tiles.push_str(&format!(
            r##"<rect x="{}" y="{}" width="18" height="18" fill="steelblue" rx="2"/>"##,
            x, y
        ));
    }

    let canvas_size = (grid_cols * 20 + 20).max(100);
    format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="{}" height="{}">{}</svg>"##,
        canvas_size, canvas_size, tiles
    )
}

/// Creates an SVG with nested repeating groups
fn create_nested_repeating_svg(group_count: usize, elements_per_group: usize) -> String {
    let mut groups = String::new();
    let grid_cols = (group_count as f64).sqrt().ceil() as usize;

    for g in 0..group_count {
        let gx = (g % grid_cols) * 100;
        let gy = (g / grid_cols) * 100;

        let mut elements = String::new();
        for e in 0..elements_per_group {
            let ex = (e % 3) * 30;
            let ey = (e / 3) * 30;
            elements.push_str(&format!(
                r##"<rect x="{}" y="{}" width="25" height="25" fill="cornflowerblue" rx="3"/>"##,
                ex, ey
            ));
        }

        groups.push_str(&format!(
            r##"<g transform="translate({},{})">{}</g>"##,
            gx, gy, elements
        ));
    }

    let canvas_size = (grid_cols * 100 + 100).max(200);
    format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="{}" height="{}">{}</svg>"##,
        canvas_size, canvas_size, groups
    )
}

/// Creates an SVG that simulates a complex dashboard with many repeated components
fn create_dashboard_svg(card_count: usize) -> String {
    let mut cards = String::new();
    let cols = 4;

    for i in 0..card_count {
        let x = (i % cols) * 250;
        let y = (i / cols) * 150;
        let value = i * 17 % 100;

        // Each card has identical structure but different data
        cards.push_str(&format!(
            concat!(
                r##"<g transform="translate({},{})">"##,
                r##"<rect width="230" height="130" fill="white" stroke="gray" rx="8"/>"##,
                r##"<rect x="10" y="10" width="210" height="40" fill="lightgray" rx="4"/>"##,
                r##"<text x="20" y="38" font-family="sans-serif" font-size="14" fill="black">Card {}</text>"##,
                r##"<rect x="10" y="60" width="{}" height="20" fill="green" rx="2"/>"##,
                r##"<text x="115" y="110" text-anchor="middle" font-family="sans-serif" font-size="24" fill="black">{}%</text>"##,
                r##"</g>"##
            ),
            x, y, i, value * 2, value
        ));
    }

    let width = cols * 250 + 50;
    let height = ((card_count + cols - 1) / cols) * 150 + 50;
    format!(
        concat!(
            r##"<svg xmlns="http://www.w3.org/2000/svg" width="{}" height="{}">"##,
            r##"<rect width="100%" height="100%" fill="whitesmoke"/>"##,
            r##"{}</svg>"##
        ),
        width, height, cards
    )
}

fn bench_repeating_elements(c: &mut Criterion) {
    let mut group = c.benchmark_group("repeating_elements");
    group.sample_size(30);

    let fontdb = fontdb::Database::new();
    let options = Options::default();
    let pixmap_pool = PixmapPool::new();

    // Test with increasing numbers of repeated elements
    for repeat_count in [100, 500, 1000, 2000] {
        let svg_data = create_repeating_svg(repeat_count, 5); // 5 unique icons repeated
        let tree = Tree::from_str(&svg_data, &options, &fontdb).unwrap();
        let size = tree.size();
        let width = size.width() as u32;
        let height = size.height() as u32;
        let pixmap_for_ctx = Pixmap::new(width, height).unwrap();
        let ctx = Context::new_from_pixmap(&pixmap_for_ctx);

        group.throughput(Throughput::Elements(repeat_count as u64));

        group.bench_with_input(
            BenchmarkId::new("with_static_cache", repeat_count),
            &repeat_count,
            |b, _| {
                let mut cache = SvgrCache::new(100);
                let mut pixmap = Pixmap::new(width, height).unwrap();

                b.iter(|| {
                    pixmap.fill(tiny_skia::Color::TRANSPARENT);
                    render(
                        black_box(&tree),
                        tiny_skia::Transform::default(),
                        &mut pixmap.as_mut(),
                        &mut cache,
                        &pixmap_pool,
                        &ctx,
                    );
                });
            },
        );

        group.bench_with_input(
            BenchmarkId::new("no_cache", repeat_count),
            &repeat_count,
            |b, _| {
                let mut cache = SvgrCache::none();
                let mut pixmap = Pixmap::new(width, height).unwrap();

                b.iter(|| {
                    pixmap.fill(tiny_skia::Color::TRANSPARENT);
                    render(
                        black_box(&tree),
                        tiny_skia::Transform::default(),
                        &mut pixmap.as_mut(),
                        &mut cache,
                        &pixmap_pool,
                        &ctx,
                    );
                });
            },
        );
    }

    group.finish();
}

fn bench_tiled_patterns(c: &mut Criterion) {
    let mut group = c.benchmark_group("tiled_patterns");
    group.sample_size(30);

    let fontdb = fontdb::Database::new();
    let options = Options::default();
    let pixmap_pool = PixmapPool::new();

    for tile_count in [400, 1600, 3600] {
        let svg_data = create_tiled_svg(tile_count);
        let tree = Tree::from_str(&svg_data, &options, &fontdb).unwrap();
        let size = tree.size();
        let width = size.width() as u32;
        let height = size.height() as u32;
        let pixmap_for_ctx = Pixmap::new(width, height).unwrap();
        let ctx = Context::new_from_pixmap(&pixmap_for_ctx);

        group.throughput(Throughput::Elements(tile_count as u64));

        group.bench_with_input(
            BenchmarkId::new("static_cache", tile_count),
            &tile_count,
            |b, _| {
                let mut cache = SvgrCache::new(50);
                let mut pixmap = Pixmap::new(width, height).unwrap();

                b.iter(|| {
                    pixmap.fill(tiny_skia::Color::TRANSPARENT);
                    render(
                        black_box(&tree),
                        tiny_skia::Transform::default(),
                        &mut pixmap.as_mut(),
                        &mut cache,
                        &pixmap_pool,
                        &ctx,
                    );
                });
            },
        );

        group.bench_with_input(
            BenchmarkId::new("no_cache", tile_count),
            &tile_count,
            |b, _| {
                let mut cache = SvgrCache::none();
                let mut pixmap = Pixmap::new(width, height).unwrap();

                b.iter(|| {
                    pixmap.fill(tiny_skia::Color::TRANSPARENT);
                    render(
                        black_box(&tree),
                        tiny_skia::Transform::default(),
                        &mut pixmap.as_mut(),
                        &mut cache,
                        &pixmap_pool,
                        &ctx,
                    );
                });
            },
        );
    }

    group.finish();
}

fn bench_nested_repeating_groups(c: &mut Criterion) {
    let mut group = c.benchmark_group("nested_repeating_groups");
    group.sample_size(30);

    let fontdb = fontdb::Database::new();
    let options = Options::default();
    let pixmap_pool = PixmapPool::new();

    for (groups, elements) in [(25, 9), (100, 9), (225, 9)] {
        let total_elements = groups * elements;
        let svg_data = create_nested_repeating_svg(groups, elements);
        let tree = Tree::from_str(&svg_data, &options, &fontdb).unwrap();
        let size = tree.size();
        let width = size.width() as u32;
        let height = size.height() as u32;
        let pixmap_for_ctx = Pixmap::new(width, height).unwrap();
        let ctx = Context::new_from_pixmap(&pixmap_for_ctx);

        group.throughput(Throughput::Elements(total_elements as u64));

        group.bench_with_input(
            BenchmarkId::new("static_cache", format!("{}g_{}e", groups, elements)),
            &total_elements,
            |b, _| {
                let mut cache = SvgrCache::new(100);
                let mut pixmap = Pixmap::new(width, height).unwrap();

                b.iter(|| {
                    pixmap.fill(tiny_skia::Color::TRANSPARENT);
                    render(
                        black_box(&tree),
                        tiny_skia::Transform::default(),
                        &mut pixmap.as_mut(),
                        &mut cache,
                        &pixmap_pool,
                        &ctx,
                    );
                });
            },
        );

        group.bench_with_input(
            BenchmarkId::new("no_cache", format!("{}g_{}e", groups, elements)),
            &total_elements,
            |b, _| {
                let mut cache = SvgrCache::none();
                let mut pixmap = Pixmap::new(width, height).unwrap();

                b.iter(|| {
                    pixmap.fill(tiny_skia::Color::TRANSPARENT);
                    render(
                        black_box(&tree),
                        tiny_skia::Transform::default(),
                        &mut pixmap.as_mut(),
                        &mut cache,
                        &pixmap_pool,
                        &ctx,
                    );
                });
            },
        );
    }

    group.finish();
}

fn bench_dashboard_simulation(c: &mut Criterion) {
    let mut group = c.benchmark_group("dashboard_simulation");
    group.sample_size(30);

    let fontdb = fontdb::Database::new();
    let options = Options::default();
    let pixmap_pool = PixmapPool::new();

    for card_count in [16, 32, 64] {
        let svg_data = create_dashboard_svg(card_count);
        let tree = Tree::from_str(&svg_data, &options, &fontdb).unwrap();
        let size = tree.size();
        let width = size.width() as u32;
        let height = size.height() as u32;
        let pixmap_for_ctx = Pixmap::new(width, height).unwrap();
        let ctx = Context::new_from_pixmap(&pixmap_for_ctx);

        group.throughput(Throughput::Elements(card_count as u64));

        group.bench_with_input(
            BenchmarkId::new("static_cache", card_count),
            &card_count,
            |b, _| {
                let mut cache = SvgrCache::new(50);
                let mut pixmap = Pixmap::new(width, height).unwrap();

                b.iter(|| {
                    pixmap.fill(tiny_skia::Color::TRANSPARENT);
                    render(
                        black_box(&tree),
                        tiny_skia::Transform::default(),
                        &mut pixmap.as_mut(),
                        &mut cache,
                        &pixmap_pool,
                        &ctx,
                    );
                });
            },
        );

        group.bench_with_input(
            BenchmarkId::new("no_cache", card_count),
            &card_count,
            |b, _| {
                let mut cache = SvgrCache::none();
                let mut pixmap = Pixmap::new(width, height).unwrap();

                b.iter(|| {
                    pixmap.fill(tiny_skia::Color::TRANSPARENT);
                    render(
                        black_box(&tree),
                        tiny_skia::Transform::default(),
                        &mut pixmap.as_mut(),
                        &mut cache,
                        &pixmap_pool,
                        &ctx,
                    );
                });
            },
        );
    }

    group.finish();
}

fn bench_cache_memory_efficiency(c: &mut Criterion) {
    let mut group = c.benchmark_group("cache_memory_efficiency");
    group.sample_size(20);

    let fontdb = fontdb::Database::new();
    let options = Options::default();
    let pixmap_pool = PixmapPool::new();

    // Test repeated rendering to measure cache reuse
    let repeat_count = 1000;
    let svg_data = create_repeating_svg(repeat_count, 3); // Only 3 unique elements repeated 1000 times
    let tree = Tree::from_str(&svg_data, &options, &fontdb).unwrap();
    let size = tree.size();
    let width = size.width() as u32;
    let height = size.height() as u32;
    let pixmap_for_ctx = Pixmap::new(width, height).unwrap();
    let ctx = Context::new_from_pixmap(&pixmap_for_ctx);

    group.bench_function("multi_frame_render_cached", |b| {
        let mut cache = SvgrCache::new(50);
        let mut pixmap = Pixmap::new(width, height).unwrap();

        b.iter(|| {
            // Simulate rendering multiple frames (cache should be warm after first)
            for _ in 0..5 {
                pixmap.fill(tiny_skia::Color::TRANSPARENT);
                render(
                    black_box(&tree),
                    tiny_skia::Transform::default(),
                    &mut pixmap.as_mut(),
                    &mut cache,
                    &pixmap_pool,
                    &ctx,
                );
            }
        });
    });

    group.bench_function("multi_frame_render_uncached", |b| {
        let mut pixmap = Pixmap::new(width, height).unwrap();

        b.iter(|| {
            for _ in 0..5 {
                let mut cache = SvgrCache::none();
                pixmap.fill(tiny_skia::Color::TRANSPARENT);
                render(
                    black_box(&tree),
                    tiny_skia::Transform::default(),
                    &mut pixmap.as_mut(),
                    &mut cache,
                    &pixmap_pool,
                    &ctx,
                );
            }
        });
    });

    group.finish();
}

fn bench_optimized_cache_config(c: &mut Criterion) {
    let mut group = c.benchmark_group("optimized_cache_config");
    group.sample_size(30);

    let fontdb = fontdb::Database::new();
    let options = Options::default();
    let pixmap_pool = PixmapPool::new();

    // Large SVG with many repeating elements
    let repeat_count = 2000;
    let svg_data = create_repeating_svg(repeat_count, 10);
    let tree = Tree::from_str(&svg_data, &options, &fontdb).unwrap();
    let size = tree.size();
    let width = size.width() as u32;
    let height = size.height() as u32;
    let pixmap_for_ctx = Pixmap::new(width, height).unwrap();
    let ctx = Context::new_from_pixmap(&pixmap_for_ctx);

    group.throughput(Throughput::Elements(repeat_count as u64));

    // Default cache configuration
    group.bench_function("default_config", |b| {
        let mut cache = SvgrCache::new(100);
        let mut pixmap = Pixmap::new(width, height).unwrap();

        b.iter(|| {
            pixmap.fill(tiny_skia::Color::TRANSPARENT);
            render(
                black_box(&tree),
                tiny_skia::Transform::default(),
                &mut pixmap.as_mut(),
                &mut cache,
                &pixmap_pool,
                &ctx,
            );
        });
    });

    // Optimized for large repeating SVGs
    group.bench_function("large_svg_optimized", |b| {
        let mut cache = SvgrCache::for_large_repeating_svgs(100);
        let mut pixmap = Pixmap::new(width, height).unwrap();

        b.iter(|| {
            pixmap.fill(tiny_skia::Color::TRANSPARENT);
            render(
                black_box(&tree),
                tiny_skia::Transform::default(),
                &mut pixmap.as_mut(),
                &mut cache,
                &pixmap_pool,
                &ctx,
            );
        });
    });

    // Custom config with larger initial capacity
    group.bench_function("custom_large_capacity", |b| {
        let config = StaticCacheConfig {
            max_bytes: 128 * 1024 * 1024,
            initial_capacity: 2048,
        };
        let mut cache = SvgrCache::with_static_config(100, config);
        let mut pixmap = Pixmap::new(width, height).unwrap();

        b.iter(|| {
            pixmap.fill(tiny_skia::Color::TRANSPARENT);
            render(
                black_box(&tree),
                tiny_skia::Transform::default(),
                &mut pixmap.as_mut(),
                &mut cache,
                &pixmap_pool,
                &ctx,
            );
        });
    });

    // Unlimited cache
    group.bench_function("unlimited_cache", |b| {
        let mut cache = SvgrCache::unlimited_static(100);
        let mut pixmap = Pixmap::new(width, height).unwrap();

        b.iter(|| {
            pixmap.fill(tiny_skia::Color::TRANSPARENT);
            render(
                black_box(&tree),
                tiny_skia::Transform::default(),
                &mut pixmap.as_mut(),
                &mut cache,
                &pixmap_pool,
                &ctx,
            );
        });
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_repeating_elements,
    bench_tiled_patterns,
    bench_nested_repeating_groups,
    bench_dashboard_simulation,
    bench_cache_memory_efficiency,
    bench_optimized_cache_config,
);
criterion_main!(benches);
