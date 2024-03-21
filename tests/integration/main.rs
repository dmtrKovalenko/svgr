use once_cell::sync::Lazy;
use rgb::FromSlice;
use std::{collections::HashMap, sync::Arc};
use svgr::SvgrCache;
use usvgr::PreloadedImageData;
use usvgr_text_layout::{fontdb, TreeTextToPath};

#[rustfmt::skip]
mod render;

const IMAGE_SIZE: u32 = 300;

static GLOBAL_FONTDB: Lazy<std::sync::Mutex<fontdb::Database>> = Lazy::new(|| {
    let mut fontdb = fontdb::Database::new();
    fontdb.load_fonts_dir("tests/fonts");
    fontdb.set_serif_family("Noto Serif");
    fontdb.set_sans_serif_family("Noto Sans");
    fontdb.set_cursive_family("Yellowtail");
    fontdb.set_fantasy_family("Sedgwick Ave Display");
    fontdb.set_monospace_family("Noto Mono");
    std::sync::Mutex::new(fontdb)
});

fn load_image(path: &str) -> Arc<PreloadedImageData> {
    let image_data = std::fs::read(path).unwrap();
    let png_image = image::load_from_memory(image_data.as_slice()).unwrap();

    Arc::new(PreloadedImageData::new(
        "png".to_owned(),
        png_image.width(),
        png_image.height(),
        &png_image.to_rgba8().into_raw(),
    ))
}

static GLOBAL_IMAGE_DATA: Lazy<Arc<HashMap<String, Arc<PreloadedImageData>>>> = Lazy::new(|| {
    let mut hash_map = HashMap::new();

    hash_map.insert("image.png".to_owned(), load_image("tests/images/image.png"));
    hash_map.insert("image.jpg".to_owned(), load_image("tests/images/image.jpg"));
    hash_map.insert(
        "image-63x61.png".to_owned(),
        load_image("tests/images/image-63x61.png"),
    );

    Arc::new(hash_map)
});

pub fn render(name: &str) -> usize {
    let svg_path = format!("tests/svg/{}.svg", name);
    let png_path = format!("tests/png/{}.png", name);

    let mut opt = usvgr::Options::default();
    opt.image_data = Some(&GLOBAL_IMAGE_DATA);
    opt.resources_dir = Some(std::path::PathBuf::from("tests/svg"));

    let tree = {
        let svg_data = std::fs::read(&svg_path).unwrap();
        let mut tree = usvgr::Tree::from_data(&svg_data, &opt).unwrap();
        let db = GLOBAL_FONTDB.lock().unwrap();
        tree.convert_text(&db, false);
        tree
    };

    let fit_to = usvgr::FitTo::Width(IMAGE_SIZE);
    let size = fit_to.fit_to(tree.size.to_screen_size()).unwrap();
    let mut pixmap = tiny_skia::Pixmap::new(size.width(), size.height()).unwrap();
    svgr::render(
        &tree,
        fit_to,
        tiny_skia::Transform::default(),
        pixmap.as_mut(),
        &mut SvgrCache::none(),
    )
    .unwrap();

    let mut rgba = pixmap.clone().take();
    svgfilters::demultiply_alpha(rgba.as_mut_slice().as_rgba_mut());

    let expected_data = load_png(&png_path);
    assert_eq!(expected_data.len(), rgba.len());

    let mut pixels_d = 0;
    for (a, b) in expected_data
        .as_slice()
        .as_rgba()
        .iter()
        .zip(rgba.as_rgba())
    {
        if is_pix_diff(*a, *b) {
            pixels_d += 1;
        }
    }

    // Save diff if needed.
    if pixels_d > 0 {
        pixmap.save_png(&format!("tests/{}.png", name)).unwrap();
        gen_diff(&name, &expected_data, rgba.as_slice()).unwrap();
    }

    pixels_d
}

fn load_png(path: &str) -> Vec<u8> {
    let data = std::fs::read(path).unwrap();
    let mut decoder = png::Decoder::new(data.as_slice());
    decoder.set_transformations(png::Transformations::normalize_to_color8());
    let mut reader = decoder.read_info().unwrap();
    let mut img_data = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut img_data).unwrap();

    match info.color_type {
        png::ColorType::Rgb => {
            panic!("RGB PNG is not supported.");
        }
        png::ColorType::Rgba => img_data,
        png::ColorType::Grayscale => {
            let mut rgba_data = Vec::with_capacity(img_data.len() * 4);
            for gray in img_data {
                rgba_data.push(gray);
                rgba_data.push(gray);
                rgba_data.push(gray);
                rgba_data.push(255);
            }

            rgba_data
        }
        png::ColorType::GrayscaleAlpha => {
            let mut rgba_data = Vec::with_capacity(img_data.len() * 2);
            for slice in img_data.chunks(2) {
                let gray = slice[0];
                let alpha = slice[1];
                rgba_data.push(gray);
                rgba_data.push(gray);
                rgba_data.push(gray);
                rgba_data.push(alpha);
            }

            rgba_data
        }
        png::ColorType::Indexed => {
            panic!("Indexed PNG is not supported.");
        }
    }
}

// TODO: remove
fn is_pix_diff(c1: rgb::RGBA8, c2: rgb::RGBA8) -> bool {
    (c1.r as i32 - c2.r as i32).abs() > 1
        || (c1.g as i32 - c2.g as i32).abs() > 1
        || (c1.b as i32 - c2.b as i32).abs() > 1
        || (c1.a as i32 - c2.a as i32).abs() > 1
}

#[allow(dead_code)]
fn gen_diff(name: &str, img1: &[u8], img2: &[u8]) -> Result<(), png::EncodingError> {
    assert_eq!(img1.len(), img2.len());

    let mut img3 = Vec::with_capacity((img1.len() as f32 * 0.75).round() as usize);
    for (a, b) in img1.as_rgba().iter().zip(img2.as_rgba()) {
        if is_pix_diff(*a, *b) {
            img3.push(255);
            img3.push(0);
            img3.push(0);
        } else {
            img3.push(255);
            img3.push(255);
            img3.push(255);
        }
    }

    let path = std::path::PathBuf::from(format!("tests/{}-diff.png", name));
    let file = std::fs::File::create(path)?;
    let ref mut w = std::io::BufWriter::new(file);

    let mut encoder = png::Encoder::new(w, IMAGE_SIZE, IMAGE_SIZE);
    encoder.set_color(png::ColorType::Rgb);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header()?;
    writer.write_image_data(&img3)
}
