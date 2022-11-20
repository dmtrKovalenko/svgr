use std::rc::Rc;

use usvgr::NodeExt;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if !(args.len() == 3 || args.len() == 5) {
        println!(
            "Usage:\n\
             \tdraw_bboxes <in-svg> <out-png>\n\
             \tdraw_bboxes <in-svg> <out-png> -z ZOOM"
        );
        return;
    }

    let zoom = if args.len() == 5 {
        args[4].parse::<f32>().expect("not a float")
    } else {
        1.0
    };

    let mut opt = usvgr::Options::default();
    // Get file's absolute directory.
    opt.resources_dir = std::fs::canonicalize(&args[1])
        .ok()
        .and_then(|p| p.parent().map(|p| p.to_path_buf()));
    opt.keep_named_groups = true;
    opt.fontdb.load_system_fonts();
    let fit_to = usvgr::FitTo::Zoom(zoom);

    let svg_data = std::fs::read(&args[1]).unwrap();
    let mut rtree = usvgr::Tree::from_data(&svg_data, &opt.to_ref()).unwrap();

    let mut bboxes = Vec::new();
    let mut text_bboxes = Vec::new();
    for node in rtree.root.descendants() {
        if let Some(bbox) = node.calculate_bbox().and_then(|r| r.to_rect()) {
            bboxes.push(bbox);
        }

        // Text bboxes are different from path bboxes.
        if let usvgr::NodeKind::Path(ref path) = *node.borrow() {
            if let Some(ref bbox) = path.text_bbox {
                text_bboxes.push(*bbox);
            }
        }
    }

    let stroke = Some(usvgr::Stroke {
        paint: usvgr::Paint::Color(usvgr::Color::new_rgb(255, 0, 0)),
        opacity: usvgr::Opacity::new_clamped(0.5),
        ..usvgr::Stroke::default()
    });

    let stroke2 = Some(usvgr::Stroke {
        paint: usvgr::Paint::Color(usvgr::Color::new_rgb(0, 0, 200)),
        opacity: usvgr::Opacity::new_clamped(0.5),
        ..usvgr::Stroke::default()
    });

    for bbox in bboxes {
        rtree.root.append_kind(usvgr::NodeKind::Path(usvgr::Path {
            stroke: stroke.clone(),
            data: Rc::new(usvgr::PathData::from_rect(bbox)),
            ..usvgr::Path::default()
        }));
    }

    for bbox in text_bboxes {
        rtree.root.append_kind(usvgr::NodeKind::Path(usvgr::Path {
            stroke: stroke2.clone(),
            data: Rc::new(usvgr::PathData::from_rect(bbox)),
            ..usvgr::Path::default()
        }));
    }

    let pixmap_size = fit_to.fit_to(rtree.size.to_screen_size()).unwrap();
    let mut pixmap = tiny_skia::Pixmap::new(pixmap_size.width(), pixmap_size.height()).unwrap();
    svgr::render(
        &rtree,
        fit_to,
        tiny_skia::Transform::default(),
        pixmap.as_mut(),
        &mut svgr::SvgrCache::none(),
    )
    .unwrap();
    pixmap.save_png(&args[2]).unwrap();
}
