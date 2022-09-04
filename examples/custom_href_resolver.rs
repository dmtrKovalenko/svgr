use std::sync::Arc;

use image;
use usvg::PreloadedImageData;

fn main() {
    let mut opt = usvg::Options::default();

    let ferris_image = std::fs::read("./examples/ferris.png").unwrap();
    let ferris_image = image::load_from_memory(ferris_image.as_slice()).unwrap();

    opt.image_data.insert(
        "ferris_image".to_string(),
        PreloadedImageData::new(
            ferris_image.width(),
            ferris_image.height(),
            ferris_image.to_rgba8().into_raw(),
        ),
    );

    let svg_data = std::fs::read("./examples/custom_href_resolver.svg").unwrap();
    let rtree = usvg::Tree::from_data(&svg_data, &opt.to_ref()).unwrap();

    let pixmap_size = rtree.svg_node().size.to_screen_size();
    let mut pixmap = tiny_skia::Pixmap::new(pixmap_size.width(), pixmap_size.height()).unwrap();

    resvg::render(
        &rtree,
        usvg::FitTo::Original,
        tiny_skia::Transform::default(),
        pixmap.as_mut(),
    )
    .unwrap();

    pixmap.save_png("custom_href_resolver.png").unwrap();
}
