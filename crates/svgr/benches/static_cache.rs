//! Benchmark for static cache optimization.
//!
//! This benchmark measures the performance improvement from using static caching
//! for compile-time known SVG elements vs LRU-only caching.
//!
//! Run with: cargo bench -p svgr --bench static_cache

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use svgr::{tiny_skia, PixmapPool, SvgrCache};
use usvgr::fontdb;

/// Create a test SVG tree with mixed static and dynamic content
fn create_test_svg(frame: usize) -> String {
    let opacity = (frame as f32 / 100.0).min(1.0);
    let x_pos = 100 + frame * 2;
    let text_x = x_pos + 50;
    let circle_x = 200 + (frame * 5) % 1520;

    format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="1920" height="1080" viewBox="0 0 1920 1080">
  <defs>
    <linearGradient id="bg-gradient" x1="0%" y1="0%" x2="100%" y2="100%">
      <stop offset="0%" stop-color="navy"/>
      <stop offset="100%" stop-color="darkblue"/>
    </linearGradient>
  </defs>
  <rect width="1920" height="1080" fill="url(#bg-gradient)"/>
  <circle cx="200" cy="200" r="150" fill="red" opacity="0.1"/>
  <circle cx="1720" cy="880" r="200" fill="red" opacity="0.08"/>
  <rect x="100" y="300" width="400" height="4" fill="purple" rx="2"/>
  <g opacity="{}">
    <rect x="{}" y="450" width="600" height="180" fill="black" rx="20"/>
    <text x="{}" y="560" font-family="sans-serif" font-size="48" fill="white">Frame {}</text>
  </g>
  <circle cx="{}" cy="800" r="30" fill="orange"/>
</svg>"#,
        opacity, x_pos, text_x, frame, circle_x
    )
}

/// Create a fully static SVG (no dynamic content)
fn create_static_svg() -> &'static str {
    r#"<svg xmlns="http://www.w3.org/2000/svg" width="1920" height="1080" viewBox="0 0 1920 1080">
  <defs>
    <linearGradient id="bg" x1="0%" y1="0%" x2="100%" y2="100%">
      <stop offset="0%" stop-color="purple"/>
      <stop offset="100%" stop-color="blue"/>
    </linearGradient>
  </defs>
  <rect width="1920" height="1080" fill="url(#bg)"/>
  <circle cx="400" cy="300" r="200" fill="white" opacity="0.3"/>
  <circle cx="1520" cy="780" r="250" fill="white" opacity="0.2"/>
  <text x="960" y="500" text-anchor="middle" font-family="sans-serif" font-size="96" font-weight="bold" fill="white">
    Fully Static Content
  </text>
  <path d="M100,700 C300,600 500,800 700,700 S1100,600 1300,700 S1700,800 1820,700" 
        stroke="white" stroke-width="4" fill="none" opacity="0.5"/>
  <rect x="760" y="600" width="400" height="100" fill="white" opacity="0.1" rx="50"/>
</svg>"#
}

fn parse_svg(svg: &str, fontdb: &fontdb::Database) -> usvgr::Tree {
    let options = usvgr::Options::default();
    usvgr::Tree::from_str(svg, &options, fontdb).unwrap()
}

fn render_frame(
    tree: &usvgr::Tree,
    pixmap: &mut tiny_skia::Pixmap,
    cache: &mut SvgrCache,
    pixmap_pool: &PixmapPool,
) {
    pixmap.fill(tiny_skia::Color::TRANSPARENT);
    let ctx = svgr::Context::new_from_pixmap(pixmap);
    svgr::render(
        tree,
        tiny_skia::Transform::identity(),
        &mut pixmap.as_mut(),
        cache,
        pixmap_pool,
        &ctx,
    );
}

fn bench_static_vs_lru_cache(c: &mut Criterion) {
    let mut group = c.benchmark_group("static_cache_comparison");
    group.sample_size(50);

    let fontdb = fontdb::Database::new();
    let pixmap_pool = PixmapPool::new();

    for num_frames in [10, 50, 100].iter() {
        group.bench_with_input(
            BenchmarkId::new("with_static_cache", num_frames),
            num_frames,
            |b, &num_frames| {
                b.iter(|| {
                    let mut cache = SvgrCache::new(50);
                    let mut pixmap = tiny_skia::Pixmap::new(1920, 1080).unwrap();

                    for frame in 0..num_frames {
                        let svg = create_test_svg(frame);
                        let tree = parse_svg(&svg, &fontdb);
                        render_frame(&tree, &mut pixmap, &mut cache, &pixmap_pool);
                        black_box(&pixmap);
                    }

                    cache.static_cache_len()
                });
            },
        );

        group.bench_with_input(
            BenchmarkId::new("lru_only", num_frames),
            num_frames,
            |b, &num_frames| {
                b.iter(|| {
                    let mut cache = SvgrCache::lru_only(50);
                    let mut pixmap = tiny_skia::Pixmap::new(1920, 1080).unwrap();

                    for frame in 0..num_frames {
                        let svg = create_test_svg(frame);
                        let tree = parse_svg(&svg, &fontdb);
                        render_frame(&tree, &mut pixmap, &mut cache, &pixmap_pool);
                        black_box(&pixmap);
                    }
                });
            },
        );

        group.bench_with_input(
            BenchmarkId::new("no_cache", num_frames),
            num_frames,
            |b, &num_frames| {
                b.iter(|| {
                    let mut cache = SvgrCache::none();
                    let mut pixmap = tiny_skia::Pixmap::new(1920, 1080).unwrap();

                    for frame in 0..num_frames {
                        let svg = create_test_svg(frame);
                        let tree = parse_svg(&svg, &fontdb);
                        render_frame(&tree, &mut pixmap, &mut cache, &pixmap_pool);
                        black_box(&pixmap);
                    }
                });
            },
        );
    }

    group.finish();
}

fn bench_fully_static_content(c: &mut Criterion) {
    let mut group = c.benchmark_group("fully_static_content");
    group.sample_size(100);

    let fontdb = fontdb::Database::new();
    let pixmap_pool = PixmapPool::new();
    let svg = create_static_svg();
    let tree = parse_svg(svg, &fontdb);
    let num_frames = 100;

    group.bench_function("with_static_cache_repeated", |b| {
        b.iter(|| {
            let mut cache = SvgrCache::new(20);
            let mut pixmap = tiny_skia::Pixmap::new(1920, 1080).unwrap();

            for _ in 0..num_frames {
                render_frame(&tree, &mut pixmap, &mut cache, &pixmap_pool);
                black_box(&pixmap);
            }

            cache.static_cache_len()
        });
    });

    group.bench_function("lru_only_repeated", |b| {
        b.iter(|| {
            let mut cache = SvgrCache::lru_only(20);
            let mut pixmap = tiny_skia::Pixmap::new(1920, 1080).unwrap();

            for _ in 0..num_frames {
                render_frame(&tree, &mut pixmap, &mut cache, &pixmap_pool);
                black_box(&pixmap);
            }
        });
    });

    group.bench_function("no_cache_repeated", |b| {
        b.iter(|| {
            let mut cache = SvgrCache::none();
            let mut pixmap = tiny_skia::Pixmap::new(1920, 1080).unwrap();

            for _ in 0..num_frames {
                render_frame(&tree, &mut pixmap, &mut cache, &pixmap_pool);
                black_box(&pixmap);
            }
        });
    });

    group.finish();
}

fn bench_cache_warmup(c: &mut Criterion) {
    let mut group = c.benchmark_group("cache_warmup");

    let fontdb = fontdb::Database::new();
    let pixmap_pool = PixmapPool::new();
    let svg = create_static_svg();
    let tree = parse_svg(svg, &fontdb);

    group.bench_function("first_render_cold_cache", |b| {
        b.iter(|| {
            let mut cache = SvgrCache::new(20);
            let mut pixmap = tiny_skia::Pixmap::new(1920, 1080).unwrap();
            render_frame(&tree, &mut pixmap, &mut cache, &pixmap_pool);
            black_box(&pixmap);
        });
    });

    group.bench_function("subsequent_render_warm_cache", |b| {
        let mut cache = SvgrCache::new(20);
        let mut pixmap = tiny_skia::Pixmap::new(1920, 1080).unwrap();
        render_frame(&tree, &mut pixmap, &mut cache, &pixmap_pool);

        b.iter(|| {
            render_frame(&tree, &mut pixmap, &mut cache, &pixmap_pool);
            black_box(&pixmap);
        });
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_static_vs_lru_cache,
    bench_fully_static_content,
    bench_cache_warmup,
);
criterion_main!(benches);
