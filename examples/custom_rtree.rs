use std::rc::Rc;

use usvgr::NodeExt;

fn main() {
    let size = usvgr::Size::new(200.0, 200.0).unwrap();
    let mut rtree = usvgr::Tree {
        size,
        view_box: usvgr::ViewBox {
            rect: size.to_rect(0.0, 0.0),
            aspect: usvgr::AspectRatio::default(),
        },
        root: usvgr::Node::new(usvgr::NodeKind::Group(usvgr::Group::default())),
    };

    let gradient = usvgr::LinearGradient {
        id: "lg1".into(),
        x1: 0.0,
        y1: 0.0,
        x2: 1.0,
        y2: 0.0,
        base: usvgr::BaseGradient {
            units: usvgr::Units::ObjectBoundingBox,
            transform: usvgr::Transform::default(),
            spread_method: usvgr::SpreadMethod::Pad,
            stops: vec![
                usvgr::Stop {
                    offset: usvgr::StopOffset::ZERO,
                    color: usvgr::Color::new_rgb(0, 255, 0),
                    opacity: usvgr::Opacity::ONE,
                },
                usvgr::Stop {
                    offset: usvgr::StopOffset::ONE,
                    color: usvgr::Color::new_rgb(0, 255, 0),
                    opacity: usvgr::Opacity::ZERO,
                },
            ],
        },
    };

    let fill = Some(usvgr::Fill {
        paint: usvgr::Paint::LinearGradient(Rc::new(gradient)),
        ..usvgr::Fill::default()
    });

    rtree.root.append_kind(usvgr::NodeKind::Path(usvgr::Path {
        fill,
        data: Rc::new(usvgr::PathData::from_rect(
            usvgr::Rect::new(20.0, 20.0, 160.0, 160.0).unwrap(),
        )),
        ..usvgr::Path::default()
    }));

    #[cfg(feature = "dump-svg")]
    {
        println!("{}", rtree.to_string(&usvgr::XmlOptions::default()));
    }

    let pixmap_size = rtree.size.to_screen_size();
    let mut pixmap = tiny_skia::Pixmap::new(pixmap_size.width(), pixmap_size.height()).unwrap();
    svgr::render(
        &rtree,
        usvgr::FitTo::Original,
        tiny_skia::Transform::default(),
        pixmap.as_mut(),
    )
    .unwrap();
    pixmap.save_png("out.png").unwrap();
}
