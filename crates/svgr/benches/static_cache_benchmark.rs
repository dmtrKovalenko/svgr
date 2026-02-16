use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use svgr::{render, Context, PixmapPool, SvgrCache};
use tiny_skia::Pixmap;
use usvgr::{fontdb, Options, Tree};

/// Creates an SVG with many static rectangles
fn create_static_svg(num_elements: usize) -> String {
    let mut elements = String::new();
    for i in 0..num_elements {
        let x = (i % 20) * 50;
        let y = (i / 20) * 50;
        let r = (i * 13) % 256;
        let g = (i * 17) % 256;
        let b = (i * 23) % 256;
        elements.push_str(&format!(
            r#"<rect x="{}" y="{}" width="40" height="40" fill="rgb({},{},{})"/>"#,
            x, y, r, g, b
        ));
    }
    format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="1000" height="1000">{}</svg>"#,
        elements
    )
}

/// Creates an SVG with nested groups containing static elements
fn create_nested_static_svg(depth: usize, elements_per_group: usize) -> String {
    fn build_group(depth: usize, elements_per_group: usize, id: &mut usize) -> String {
        if depth == 0 {
            let mut elements = String::new();
            for _ in 0..elements_per_group {
                let x = (*id % 20) * 50;
                let y = (*id / 20) * 50;
                let r = (*id * 13) % 256;
                let g = (*id * 17) % 256;
                let b = (*id * 23) % 256;
                elements.push_str(&format!(
                    r#"<rect x="{}" y="{}" width="40" height="40" fill="rgb({},{},{})"/>"#,
                    x, y, r, g, b
                ));
                *id += 1;
            }
            elements
        } else {
            let mut groups = String::new();
            for _ in 0..2 {
                let inner = build_group(depth - 1, elements_per_group, id);
                groups.push_str(&format!(r#"<g>{}</g>"#, inner));
            }
            groups
        }
    }

    let mut id = 0;
    let content = build_group(depth, elements_per_group, &mut id);
    format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="1000" height="1000">{}</svg>"#,
        content
    )
}

fn bench_static_cache(c: &mut Criterion) {
    let mut group = c.benchmark_group("static_cache");

    let fontdb = fontdb::Database::new();
    let options = Options::default();
    let pixmap_pool = PixmapPool::new();

    // Test with different numbers of elements
    for num_elements in [100, 500, 1000] {
        let svg_data = create_static_svg(num_elements);
        let tree = Tree::from_str(&svg_data, &options, &fontdb).unwrap();
        let size = tree.size();
        let pixmap_for_ctx = Pixmap::new(size.width() as u32, size.height() as u32).unwrap();
        let ctx = Context::new_from_pixmap(&pixmap_for_ctx);

        // Benchmark with static cache enabled
        group.bench_with_input(
            BenchmarkId::new("with_static_cache", num_elements),
            &num_elements,
            |b, _| {
                let mut cache = SvgrCache::new(100);
                let mut pixmap = Pixmap::new(size.width() as u32, size.height() as u32).unwrap();

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

        // Benchmark with LRU only (no static cache)
        group.bench_with_input(
            BenchmarkId::new("lru_only", num_elements),
            &num_elements,
            |b, _| {
                let mut cache = SvgrCache::lru_only(100);
                let mut pixmap = Pixmap::new(size.width() as u32, size.height() as u32).unwrap();

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

        // Benchmark with no cache at all
        group.bench_with_input(
            BenchmarkId::new("no_cache", num_elements),
            &num_elements,
            |b, _| {
                let mut cache = SvgrCache::none();
                let mut pixmap = Pixmap::new(size.width() as u32, size.height() as u32).unwrap();

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

fn bench_nested_groups(c: &mut Criterion) {
    let mut group = c.benchmark_group("nested_groups");

    let fontdb = fontdb::Database::new();
    let options = Options::default();
    let pixmap_pool = PixmapPool::new();

    // Test with different nesting depths
    for depth in [2, 3, 4] {
        let svg_data = create_nested_static_svg(depth, 4);
        let tree = Tree::from_str(&svg_data, &options, &fontdb).unwrap();
        let size = tree.size();
        let num_elements = 4 * (1 << depth); // 4 * 2^depth elements
        let pixmap_for_ctx = Pixmap::new(size.width() as u32, size.height() as u32).unwrap();
        let ctx = Context::new_from_pixmap(&pixmap_for_ctx);

        group.bench_with_input(
            BenchmarkId::new(
                "with_static_cache",
                format!("depth_{}_elems_{}", depth, num_elements),
            ),
            &depth,
            |b, _| {
                let mut cache = SvgrCache::new(100);
                let mut pixmap = Pixmap::new(size.width() as u32, size.height() as u32).unwrap();

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
            BenchmarkId::new(
                "no_cache",
                format!("depth_{}_elems_{}", depth, num_elements),
            ),
            &depth,
            |b, _| {
                let mut cache = SvgrCache::none();
                let mut pixmap = Pixmap::new(size.width() as u32, size.height() as u32).unwrap();

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

criterion_group!(benches, bench_static_cache, bench_nested_groups);
criterion_main!(benches);
