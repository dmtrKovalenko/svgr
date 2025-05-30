use svgr::SvgrCache;
use usvgr::fontdb;

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

    let mut fontdb = fontdb::Database::new();
    fontdb.load_system_fonts();

    let svg_data = std::fs::read(&args[1]).unwrap();
    let tree = usvgr::Tree::from_data(&svg_data, &opt, &fontdb).unwrap();

    let mut bboxes = Vec::new();
    let mut stroke_bboxes = Vec::new();
    collect_bboxes(tree.root(), &mut bboxes, &mut stroke_bboxes);

    let pixmap_size = tree.size().to_int_size().scale_by(zoom).unwrap();
    let mut pixmap = tiny_skia::Pixmap::new(pixmap_size.width(), pixmap_size.height()).unwrap();
    let render_ts = tiny_skia::Transform::from_scale(zoom, zoom);
    let ctx = svgr::Context::new_from_pixmap(&pixmap);
    svgr::render(
        &tree,
        render_ts,
        &mut pixmap.as_mut(),
        &mut SvgrCache::none(),
        &mut svgr::PixmapPool::new(),
        &ctx,
    );

    #[allow(clippy::field_reassign_with_default)]
    let mut stroke = tiny_skia::Stroke::default();
    stroke.width = 1.0 / zoom; // prevent stroke scaling as well

    let mut paint1 = tiny_skia::Paint::default();
    paint1.set_color_rgba8(255, 0, 0, 127);

    let mut paint2 = tiny_skia::Paint::default();
    paint2.set_color_rgba8(0, 200, 0, 127);

    let root_ts = tree.view_box().to_transform(tree.size());
    let bbox_ts = render_ts.pre_concat(root_ts);

    for bbox in bboxes {
        let path = tiny_skia::PathBuilder::from_rect(bbox);
        pixmap.stroke_path(&path, &paint1, &stroke, bbox_ts, None);
    }

    for bbox in stroke_bboxes {
        let path = tiny_skia::PathBuilder::from_rect(bbox);
        pixmap.stroke_path(&path, &paint2, &stroke, bbox_ts, None);
    }

    pixmap.save_png(&args[2]).unwrap();
}

fn collect_bboxes(
    parent: &usvgr::Group,
    bboxes: &mut Vec<usvgr::Rect>,
    stroke_bboxes: &mut Vec<usvgr::Rect>,
) {
    for node in parent.children() {
        if let usvgr::Node::Group(ref group) = node {
            collect_bboxes(group, bboxes, stroke_bboxes);
        }

        let bbox = node.abs_bounding_box();
        bboxes.push(bbox);

        let stroke_bbox = node.abs_stroke_bounding_box();
        if bbox != stroke_bbox {
            stroke_bboxes.push(stroke_bbox);
        }
    }
}
