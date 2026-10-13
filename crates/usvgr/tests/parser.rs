use usvgr::Color;

#[test]
fn gradient_stop_offset_overflowing_f32() {
    // `4e38` overflows f32 to infinity; parsing must not panic.
    let svg = "<svg xmlns='http://www.w3.org/2000/svg'>\
        <defs><linearGradient id='g'><stop offset='4e38'/></linearGradient></defs>\
        <rect width='1' height='1' fill='url(#g)'/>\
    </svg>";
    let fontdb = usvgr::fontdb::Database::new();
    assert!(usvgr::Tree::from_str(svg, &usvgr::Options::default(), &fontdb).is_ok());
}

#[test]
fn clippath_with_invalid_child() {
    let svg = "
    <svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 1 1'>
        <clipPath id='clip1'>
            <rect/>
        </clipPath>
        <rect clip-path='url(#clip1)' width='10' height='10'/>
    </svg>
    ";

    let fontdb = usvgr::fontdb::Database::new();
    let tree = usvgr::Tree::from_str(&svg, &usvgr::Options::default(), &fontdb).unwrap();
    // clipPath is invalid and should be removed together with rect.
    assert_eq!(tree.root().has_children(), false);
}

#[test]
fn stylesheet_injection() {
    let svg = "<svg id='svg1' viewBox='0 0 200 200' xmlns='http://www.w3.org/2000/svg'>
    <style>
        #rect4 {
            fill: green
        }
    </style>
    <rect id='rect1' x='20' y='20' width='60' height='60'/>
    <rect id='rect2' x='120' y='20' width='60' height='60' fill='green'/>
    <rect id='rect3' x='20' y='120' width='60' height='60' style='fill: green'/>
    <rect id='rect4' x='120' y='120' width='60' height='60'/>
    <rect id='rect5' x='70' y='70' width='60' height='60' style='fill: green !important'/>
</svg>
";

    let stylesheet = "rect { fill: red }".to_string();

    let options = usvgr::Options {
        style_sheet: Some(stylesheet),
        ..usvgr::Options::default()
    };

    let fontdb = usvgr::fontdb::Database::new();
    let tree = usvgr::Tree::from_str(&svg, &options, &fontdb).unwrap();

    let usvgr::Node::Path(ref first) = &tree.root().children()[0] else {
        unreachable!()
    };

    // Only the rects with no CSS attributes should be overridden.
    assert_eq!(
        first.fill().unwrap().paint(),
        &usvgr::Paint::Color(Color::new_rgb(255, 0, 0))
    );

    let usvgr::Node::Path(ref second) = &tree.root().children()[1] else {
        unreachable!()
    };
    assert_eq!(
        second.fill().unwrap().paint(),
        &usvgr::Paint::Color(Color::new_rgb(255, 0, 0))
    );

    let usvgr::Node::Path(ref third) = &tree.root().children()[2] else {
        unreachable!()
    };
    assert_eq!(
        third.fill().unwrap().paint(),
        &usvgr::Paint::Color(Color::new_rgb(0, 128, 0))
    );

    let usvgr::Node::Path(ref third) = &tree.root().children()[3] else {
        unreachable!()
    };
    assert_eq!(
        third.fill().unwrap().paint(),
        &usvgr::Paint::Color(Color::new_rgb(0, 128, 0))
    );

    let usvgr::Node::Path(ref third) = &tree.root().children()[4] else {
        unreachable!()
    };
    assert_eq!(
        third.fill().unwrap().paint(),
        &usvgr::Paint::Color(Color::new_rgb(0, 128, 0))
    );
}

#[test]
fn stylesheet_injection_with_important() {
    let svg = "<svg id='svg1' viewBox='0 0 200 200' xmlns='http://www.w3.org/2000/svg'>
    <style>
        #rect4 {
            fill: green
        }
    </style>
    <rect id='rect1' x='20' y='20' width='60' height='60'/>
    <rect id='rect2' x='120' y='20' width='60' height='60' fill='green'/>
    <rect id='rect3' x='20' y='120' width='60' height='60' style='fill: green'/>
    <rect id='rect4' x='120' y='120' width='60' height='60'/>
    <rect id='rect5' x='70' y='70' width='60' height='60' style='fill: green !important'/>
</svg>
";

    let stylesheet = "rect { fill: red !important }".to_string();

    let options = usvgr::Options {
        style_sheet: Some(stylesheet),
        ..usvgr::Options::default()
    };

    let fontdb = usvgr::fontdb::Database::new();
    let tree = usvgr::Tree::from_str(&svg, &options, &fontdb).unwrap();

    // All rects should be overriden, since we use `important`.
    let usvgr::Node::Path(ref third) = &tree.root().children()[0] else {
        unreachable!()
    };
    assert_eq!(
        third.fill().unwrap().paint(),
        &usvgr::Paint::Color(Color::new_rgb(255, 0, 0))
    );
    let usvgr::Node::Path(ref third) = &tree.root().children()[1] else {
        unreachable!()
    };
    assert_eq!(
        third.fill().unwrap().paint(),
        &usvgr::Paint::Color(Color::new_rgb(255, 0, 0))
    );
    let usvgr::Node::Path(ref third) = &tree.root().children()[2] else {
        unreachable!()
    };
    assert_eq!(
        third.fill().unwrap().paint(),
        &usvgr::Paint::Color(Color::new_rgb(255, 0, 0))
    );
    let usvgr::Node::Path(ref third) = &tree.root().children()[3] else {
        unreachable!()
    };
    assert_eq!(
        third.fill().unwrap().paint(),
        &usvgr::Paint::Color(Color::new_rgb(255, 0, 0))
    );
    let usvgr::Node::Path(ref third) = &tree.root().children()[4] else {
        unreachable!()
    };
    assert_eq!(
        third.fill().unwrap().paint(),
        &usvgr::Paint::Color(Color::new_rgb(255, 0, 0))
    );
}

#[test]
fn simplify_paths() {
    let svg = "
    <svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 1 1'>
        <path d='M 10 20 L 10 30 Z Z Z'/>
    </svg>
    ";

    let fontdb = usvgr::fontdb::Database::new();
    let tree = usvgr::Tree::from_str(&svg, &usvgr::Options::default(), &fontdb).unwrap();
    let path = &tree.root().children()[0];
    match path {
        usvgr::Node::Path(ref path) => {
            // Make sure we have MLZ and not MLZZZ
            assert_eq!(path.data().verbs().len(), 3);
        }
        _ => unreachable!(),
    };
}

#[test]
fn size_detection_1() {
    let svg = "<svg viewBox='0 0 10 20' xmlns='http://www.w3.org/2000/svg'/>";
    let fontdb = usvgr::fontdb::Database::new();
    let tree = usvgr::Tree::from_str(&svg, &usvgr::Options::default(), &fontdb).unwrap();
    assert_eq!(tree.size(), usvgr::Size::from_wh(10.0, 20.0).unwrap());
}

#[test]
fn size_detection_2() {
    let svg =
        "<svg width='30' height='40' viewBox='0 0 10 20' xmlns='http://www.w3.org/2000/svg'/>";
    let fontdb = usvgr::fontdb::Database::new();
    let tree = usvgr::Tree::from_str(&svg, &usvgr::Options::default(), &fontdb).unwrap();
    assert_eq!(tree.size(), usvgr::Size::from_wh(30.0, 40.0).unwrap());
}

#[test]
fn size_detection_3() {
    let svg =
        "<svg width='50%' height='100%' viewBox='0 0 10 20' xmlns='http://www.w3.org/2000/svg'/>";
    let fontdb = usvgr::fontdb::Database::new();
    let tree = usvgr::Tree::from_str(&svg, &usvgr::Options::default(), &fontdb).unwrap();
    assert_eq!(tree.size(), usvgr::Size::from_wh(5.0, 20.0).unwrap());
}

#[test]
fn size_detection_4() {
    let svg = "
    <svg xmlns='http://www.w3.org/2000/svg'>
        <circle cx='18' cy='18' r='18'/>
    </svg>
    ";
    let fontdb = usvgr::fontdb::Database::new();
    let tree = usvgr::Tree::from_str(&svg, &usvgr::Options::default(), &fontdb).unwrap();
    assert_eq!(tree.size(), usvgr::Size::from_wh(36.0, 36.0).unwrap());
    assert_eq!(
        tree.view_box().rect,
        usvgr::NonZeroRect::from_xywh(0.0, 0.0, 36.0, 36.0).unwrap()
    );
}

#[test]
fn size_detection_5() {
    let svg = "<svg xmlns='http://www.w3.org/2000/svg'/>";
    let fontdb = usvgr::fontdb::Database::new();
    let tree = usvgr::Tree::from_str(&svg, &usvgr::Options::default(), &fontdb).unwrap();
    assert_eq!(tree.size(), usvgr::Size::from_wh(100.0, 100.0).unwrap());
}

#[test]
fn size_detection_6() {
    let svg = "<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 10 10' width='30'/>";
    let fontdb = usvgr::fontdb::Database::new();
    let tree = usvgr::Tree::from_str(&svg, &usvgr::Options::default(), &fontdb).unwrap();
    assert_eq!(tree.size(), usvgr::Size::from_wh(30.0, 30.0).unwrap());
}

#[test]
fn size_detection_7() {
    let svg = "<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 10 20' width='30'/>";
    let fontdb = usvgr::fontdb::Database::new();
    let tree = usvgr::Tree::from_str(&svg, &usvgr::Options::default(), &fontdb).unwrap();
    assert_eq!(tree.size(), usvgr::Size::from_wh(30.0, 60.0).unwrap());
}

#[test]
fn size_detection_8() {
    let svg = "<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 10 10' height='30'/>";
    let fontdb = usvgr::fontdb::Database::new();
    let tree = usvgr::Tree::from_str(&svg, &usvgr::Options::default(), &fontdb).unwrap();
    assert_eq!(tree.size(), usvgr::Size::from_wh(30.0, 30.0).unwrap());
}

#[test]
fn size_detection_9() {
    let svg = "<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 10 20' height='30'/>";
    let fontdb = usvgr::fontdb::Database::new();
    let tree = usvgr::Tree::from_str(&svg, &usvgr::Options::default(), &fontdb).unwrap();
    assert_eq!(tree.size(), usvgr::Size::from_wh(15.0, 30.0).unwrap());
}

#[test]
fn invalid_size_1() {
    let svg = "<svg width='0' height='0' viewBox='0 0 10 20' xmlns='http://www.w3.org/2000/svg'/>";
    let fontdb = usvgr::fontdb::Database::new();
    let result = usvgr::Tree::from_str(&svg, &usvgr::Options::default(), &fontdb);
    assert!(result.is_err());
}

#[test]
fn tree_is_send_and_sync() {
    fn ensure_send_and_sync<T: Send + Sync>() {}
    ensure_send_and_sync::<usvgr::Tree>();
}

#[test]
fn path_transform() {
    let svg = "
    <svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 100 100'>
        <path transform='translate(10)' d='M 0 0 L 10 10'/>
    </svg>
    ";

    let fontdb = usvgr::fontdb::Database::new();
    let tree = usvgr::Tree::from_str(&svg, &usvgr::Options::default(), &fontdb).unwrap();
    assert_eq!(tree.root().children().len(), 1);

    let group_node = &tree.root().children()[0];
    assert!(matches!(group_node, usvgr::Node::Group(_)));
    assert_eq!(
        group_node.abs_transform(),
        usvgr::Transform::from_translate(10.0, 0.0)
    );

    let group = match group_node {
        usvgr::Node::Group(ref g) => g,
        _ => unreachable!(),
    };

    let path = &group.children()[0];
    assert!(matches!(path, usvgr::Node::Path(_)));
    assert_eq!(
        path.abs_transform(),
        usvgr::Transform::from_translate(10.0, 0.0)
    );
}

#[test]
fn path_transform_nested() {
    let svg = "
    <svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 100 100'>
        <g transform='translate(20)'>
            <path transform='translate(10)' d='M 0 0 L 10 10'/>
        </g>
    </svg>
    ";

    let fontdb = usvgr::fontdb::Database::new();
    let tree = usvgr::Tree::from_str(&svg, &usvgr::Options::default(), &fontdb).unwrap();
    assert_eq!(tree.root().children().len(), 1);

    let group_node1 = &tree.root().children()[0];
    assert!(matches!(group_node1, usvgr::Node::Group(_)));
    assert_eq!(
        group_node1.abs_transform(),
        usvgr::Transform::from_translate(20.0, 0.0)
    );

    let group1 = match group_node1 {
        usvgr::Node::Group(ref g) => g,
        _ => unreachable!(),
    };

    let group_node2 = &group1.children()[0];
    assert!(matches!(group_node2, usvgr::Node::Group(_)));
    assert_eq!(
        group_node2.abs_transform(),
        usvgr::Transform::from_translate(30.0, 0.0)
    );

    let group2 = match group_node2 {
        usvgr::Node::Group(ref g) => g,
        _ => unreachable!(),
    };

    let path = &group2.children()[0];
    assert!(matches!(path, usvgr::Node::Path(_)));
    assert_eq!(
        path.abs_transform(),
        usvgr::Transform::from_translate(30.0, 0.0)
    );
}

#[test]
fn path_transform_in_symbol_no_clip() {
    let svg = "
    <svg viewBox='0 0 100 100' xmlns='http://www.w3.org/2000/svg' xmlns:xlink='http://www.w3.org/1999/xlink'>
        <defs>
            <symbol id='symbol1' overflow='visible'>
                <rect id='rect1' x='0' y='0' width='10' height='10'/>
            </symbol>
        </defs>
        <use id='use1' xlink:href='#symbol1' x='20'/>
    </svg>
    ";

    // Will be parsed as:
    // <svg width="100" height="100" viewBox="0 0 100 100" xmlns="http://www.w3.org/2000/svg">
    //     <g id="use1">
    //         <g transform="matrix(1 0 0 1 20 0)">
    //             <path fill="#000000" stroke="none" d="M 0 0 L 10 0 L 10 10 L 0 10 Z"/>
    //         </g>
    //     </g>
    // </svg>

    let fontdb = usvgr::fontdb::Database::new();
    let tree = usvgr::Tree::from_str(&svg, &usvgr::Options::default(), &fontdb).unwrap();

    let group_node1 = &tree.root().children()[0];
    assert!(matches!(group_node1, usvgr::Node::Group(_)));
    assert_eq!(group_node1.id(), "use1");
    assert_eq!(group_node1.abs_transform(), usvgr::Transform::default());

    let group1 = match group_node1 {
        usvgr::Node::Group(ref g) => g,
        _ => unreachable!(),
    };

    let group_node2 = &group1.children()[0];
    assert!(matches!(group_node2, usvgr::Node::Group(_)));
    assert_eq!(
        group_node2.abs_transform(),
        usvgr::Transform::from_translate(20.0, 0.0)
    );

    let group2 = match group_node2 {
        usvgr::Node::Group(ref g) => g,
        _ => unreachable!(),
    };

    let path = &group2.children()[0];
    assert!(matches!(path, usvgr::Node::Path(_)));
    assert_eq!(
        path.abs_transform(),
        usvgr::Transform::from_translate(20.0, 0.0)
    );
}

#[test]
fn path_transform_in_symbol_with_clip() {
    let svg = "
    <svg viewBox='0 0 100 100' xmlns='http://www.w3.org/2000/svg' xmlns:xlink='http://www.w3.org/1999/xlink'>
        <defs>
            <symbol id='symbol1' overflow='hidden'>
                <rect id='rect1' x='0' y='0' width='10' height='10'/>
            </symbol>
        </defs>
        <use id='use1' xlink:href='#symbol1' x='20'/>
    </svg>
    ";

    // Will be parsed as:
    // <svg width="100" height="100" viewBox="0 0 100 100" xmlns="http://www.w3.org/2000/svg">
    //     <defs>
    //         <clipPath id="clipPath1">
    //             <path fill="#000000" stroke="none" d="M 20 0 L 120 0 L 120 100 L 20 100 Z"/>
    //         </clipPath>
    //     </defs>
    //     <g id="use1" clip-path="url(#clipPath1)">
    //         <g>
    //             <g transform="matrix(1 0 0 1 20 0)">
    //                 <path fill="#000000" stroke="none" d="M 0 0 L 10 0 L 10 10 L 0 10 Z"/>
    //             </g>
    //         </g>
    //     </g>
    // </svg>

    let fontdb = usvgr::fontdb::Database::new();
    let tree = usvgr::Tree::from_str(&svg, &usvgr::Options::default(), &fontdb).unwrap();

    let group_node1 = &tree.root().children()[0];
    assert!(matches!(group_node1, usvgr::Node::Group(_)));
    assert_eq!(group_node1.id(), "use1");
    assert_eq!(group_node1.abs_transform(), usvgr::Transform::default());

    let group1 = match group_node1 {
        usvgr::Node::Group(ref g) => g,
        _ => unreachable!(),
    };

    let group_node2 = &group1.children()[0];
    assert!(matches!(group_node2, usvgr::Node::Group(_)));
    assert_eq!(group_node2.abs_transform(), usvgr::Transform::default());

    let group2 = match group_node2 {
        usvgr::Node::Group(ref g) => g,
        _ => unreachable!(),
    };

    let group_node3 = &group2.children()[0];
    assert!(matches!(group_node3, usvgr::Node::Group(_)));
    assert_eq!(
        group_node3.abs_transform(),
        usvgr::Transform::from_translate(20.0, 0.0)
    );

    let group3 = match group_node3 {
        usvgr::Node::Group(ref g) => g,
        _ => unreachable!(),
    };

    let path = &group3.children()[0];
    assert!(matches!(path, usvgr::Node::Path(_)));
    assert_eq!(
        path.abs_transform(),
        usvgr::Transform::from_translate(20.0, 0.0)
    );
}

#[test]
fn path_transform_in_svg() {
    let svg = "
    <svg viewBox='0 0 100 100' xmlns='http://www.w3.org/2000/svg' xmlns:xlink='http://www.w3.org/1999/xlink'>
        <g id='g1' transform='translate(100 150)'>
            <svg id='svg1' width='100' height='50'>
                <rect id='rect1' width='10' height='10'/>
            </svg>
        </g>
    </svg>
    ";

    // Will be parsed as:
    // <svg width="100" height="100" viewBox="0 0 100 100" xmlns="http://www.w3.org/2000/svg">
    //     <defs>
    //         <clipPath id="clipPath1">
    //             <path fill="#000000" stroke="none" d="M 0 0 L 100 0 L 100 50 L 0 50 Z"/>
    //         </clipPath>
    //     </defs>
    //     <g id="g1" transform="matrix(1 0 0 1 100 150)">
    //         <g id="svg1" clip-path="url(#clipPath1)">
    //             <path id="rect1" fill="#000000" stroke="none" d="M 0 0 L 10 0 L 10 10 L 0 10 Z"/>
    //         </g>
    //     </g>
    // </svg>

    let fontdb = usvgr::fontdb::Database::new();
    let tree = usvgr::Tree::from_str(&svg, &usvgr::Options::default(), &fontdb).unwrap();

    let group_node1 = &tree.root().children()[0];
    assert!(matches!(group_node1, usvgr::Node::Group(_)));
    assert_eq!(group_node1.id(), "g1");
    assert_eq!(
        group_node1.abs_transform(),
        usvgr::Transform::from_translate(100.0, 150.0)
    );

    let group1 = match group_node1 {
        usvgr::Node::Group(ref g) => g,
        _ => unreachable!(),
    };

    let group_node2 = &group1.children()[0];
    assert!(matches!(group_node2, usvgr::Node::Group(_)));
    assert_eq!(group_node2.id(), "svg1");
    assert_eq!(
        group_node2.abs_transform(),
        usvgr::Transform::from_translate(100.0, 150.0)
    );

    let group2 = match group_node2 {
        usvgr::Node::Group(ref g) => g,
        _ => unreachable!(),
    };

    let path = &group2.children()[0];
    assert!(matches!(path, usvgr::Node::Path(_)));
    assert_eq!(
        path.abs_transform(),
        usvgr::Transform::from_translate(100.0, 150.0)
    );
}

#[test]
fn custom_font_resolver() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    let svg = "
    <svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 100 100'>
        <text x='10' y='50' font-family='Noto Sans'>Text</text>
    </svg>
    ";

    let mut fontdb = usvgr::fontdb::Database::new();
    fontdb.load_fonts_dir("../svgr/tests/fonts");

    // The default resolver finds the font.
    let tree = usvgr::Tree::from_str(&svg, &usvgr::Options::default(), &fontdb).unwrap();
    assert!(tree.root().has_children());

    // A resolver that never selects a font produces no text.
    let calls = Arc::new(AtomicUsize::new(0));
    let calls2 = calls.clone();
    let opt = usvgr::Options {
        font_resolver: usvgr::FontResolver {
            select_font: Box::new(move |_, _| {
                calls2.fetch_add(1, Ordering::SeqCst);
                None
            }),
            select_fallback: usvgr::FontResolver::default_fallback_selector(),
        },
        ..usvgr::Options::default()
    };
    let tree = usvgr::Tree::from_str(&svg, &opt, &fontdb).unwrap();
    assert!(calls.load(Ordering::SeqCst) > 0);
    assert!(!tree.root().has_children());
}

#[test]
fn svgtree_names_roundtrip() {
    // Guards the generated perfect-hash maps in svgtree/names.rs.
    let attributes = std::fs::read_to_string("codegen/attributes.txt").unwrap();
    for name in attributes.lines().filter(|s| !s.is_empty()) {
        let aid = usvgr::svgtree::AId::from_str(name).expect(name);
        assert_eq!(aid.to_str(), name);
    }

    let elements = std::fs::read_to_string("codegen/elements.txt").unwrap();
    for name in elements.lines().filter(|s| !s.is_empty()) {
        let eid = usvgr::svgtree::EId::from_str(name).expect(name);
        assert_eq!(eid.to_str(), name);
    }
}

#[test]
fn no_text_nodes() {
    let svg = "
    <svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 100 100'>
        <g transform='translate(20)'>
            <path transform='translate(10)' d='M 0 0 L 10 10'/>
        </g>
    </svg>
    ";

    let fontdb = usvgr::fontdb::Database::new();
    let tree = usvgr::Tree::from_str(&svg, &usvgr::Options::default(), &fontdb).unwrap();
    assert!(!tree.has_text_nodes());
}

#[test]
fn flattened_text_should_inherit_absolute_transform() {
    let svg = "
    <svg viewBox='0 0 200 200' xmlns='http://www.w3.org/2000/svg'>
        <g transform='translate(20 20)'>
            <g transform='translate(20 20)'>
                <text x='32' y='100'>Text</text>
            </g>
        </g>
    </svg>
    ";

    let mut fontdb = usvgr::fontdb::Database::new();
    fontdb.load_fonts_dir(env!("CARGO_MANIFEST_DIR").to_string() + "/../svgr/tests/fonts");
    let mut opts = usvgr::Options::default();
    opts.font_family = "Noto Sans".to_string();

    let tree = usvgr::Tree::from_str(&svg, &opts, &fontdb).unwrap();

    let usvgr::Node::Group(group0) = &tree.root().children()[0] else {
        unreachable!()
    };
    let usvgr::Node::Group(group1) = &group0.children()[0] else {
        unreachable!()
    };
    let usvgr::Node::Text(text) = &group1.children()[0] else {
        unreachable!()
    };
    let usvgr::Node::Path(path) = &text.flattened().children()[0] else {
        unreachable!()
    };

    let t = path.abs_transform();

    assert_eq!(t.tx, 40.0);
    assert_eq!(t.ty, 40.0);

    assert_ne!(path.bounding_box(), path.abs_bounding_box());
    assert_eq!(
        path.bounding_box().transform(t).unwrap(),
        path.abs_bounding_box()
    );
}

#[test]
fn use_node_abs_transform() {
    let svg = "
    <svg viewBox='0 0 200 200'
         xmlns='http://www.w3.org/2000/svg'
         xmlns:xlink='http://www.w3.org/1999/xlink'>
        <defs>
            <rect id='rect1' x='0' y='0' width='100' height='100'/>
        </defs>
        <use xlink:href='#rect1' transform='matrix(0.5, 0, 0, 0.5, 20, 30)' />
    </svg>
    ";

    let fontdb = usvgr::fontdb::Database::new();
    let tree = usvgr::Tree::from_str(&svg, &usvgr::Options::default(), &fontdb).unwrap();

    let usvgr::Node::Group(group_node) = &tree.root().children()[0] else {
        unreachable!()
    };
    assert_eq!(group_node.abs_transform().get_scale(), (0.5, 0.5));

    let usvgr::Node::Path(path_node) = &group_node.children()[0] else {
        unreachable!()
    };
    assert_eq!(path_node.abs_transform().get_scale(), (0.5, 0.5));
    assert_eq!(
        path_node.abs_bounding_box(),
        usvgr::Rect::from_xywh(20.0, 30.0, 50.0, 50.0).unwrap()
    );
}

#[test]
fn nested_svg_abs_bounding_box() {
    let svg = "
    <svg xmlns='http://www.w3.org/2000/svg' width='100' height='100'>
        <svg transform='translate(10, 20)' width='50' height='50'>
            <path id='path' d='M 0 0 L 10 10'/>
        </svg>
    </svg>
    ";
    let fontdb = usvgr::fontdb::Database::new();
    let tree = usvgr::Tree::from_str(&svg, &usvgr::Options::default(), &fontdb).unwrap();
    assert_eq!(
        tree.node_by_id("path").unwrap().abs_bounding_box(),
        usvgr::Rect::from_xywh(10.0, 20.0, 10.0, 10.0).unwrap()
    );
}

#[test]
fn resolve_fr_from_referenced_radial_gradient() {
    let svg = "
    <svg viewBox='0 0 100 100' xmlns='http://www.w3.org/2000/svg' xmlns:xlink='http://www.w3.org/1999/xlink'>
        <defs>
            <radialGradient id='base' gradientUnits='userSpaceOnUse' cx='50' cy='50' r='50' fx='50' fy='50' fr='25'>
                <stop offset='0%' stop-color='red'/>
                <stop offset='100%' stop-color='blue'/>
            </radialGradient>
            <radialGradient id='ref' href='#base' xlink:href='#base'/>
        </defs>
        <rect width='100' height='100' fill='url(#ref)'/>
    </svg>
    ";

    let fontdb = usvgr::fontdb::Database::new();
    let tree = usvgr::Tree::from_str(svg, &usvgr::Options::default(), &fontdb).unwrap();

    let usvgr::Node::Path(path) = &tree.root().children()[0] else {
        unreachable!()
    };
    let usvgr::Paint::RadialGradient(rg) = path.fill().unwrap().paint() else {
        unreachable!()
    };

    assert_eq!(rg.fr().get(), 25.0);
}

#[test]
fn background_color_does_not_change_detected_size() {
    let svg = "
    <svg xmlns='http://www.w3.org/2000/svg' background-color='red'>
        <rect width='20' height='30'/>
    </svg>
    ";

    let fontdb = usvgr::fontdb::Database::new();
    let tree = usvgr::Tree::from_str(svg, &usvgr::Options::default(), &fontdb).unwrap();
    assert_eq!(tree.size(), usvgr::Size::from_wh(20.0, 30.0).unwrap());

    // The background covers the detected size.
    let usvgr::Node::Path(background) = &tree.root().children()[0] else {
        unreachable!()
    };
    assert_eq!(
        background.bounding_box(),
        usvgr::Rect::from_xywh(0.0, 0.0, 20.0, 30.0).unwrap()
    );
}

mod numbers_set_from_code {
    //! A number set from code (fframes' `svgr!` with an expression) is stored as
    //! `Float(value, "")`: it has no text. Readers must take the number, not the empty text,
    //! so it renders exactly as the same number written in the SVG (`Float(value, "value")`).

    use usvgr::svgtree::{
        AId, Attribute, EId, NestedNodeData, NestedNodeKind, NestedSvgDocument, StringStorage,
        SvgAttributeValue,
    };

    type Attrs = Vec<(AId, SvgAttributeValue<'static>)>;

    fn el(
        tag_name: EId,
        attrs: Attrs,
        children: Vec<NestedNodeData<'static>>,
    ) -> NestedNodeData<'static> {
        NestedNodeData {
            kind: NestedNodeKind::Element { tag_name },
            attrs: attrs
                .into_iter()
                .map(|(name, value)| Attribute { name, value })
                .collect(),
            children: children.into_iter().map(Some).collect(),
            static_hash: None,
        }
    }

    fn text(content: &'static str) -> NestedNodeData<'static> {
        NestedNodeData {
            kind: NestedNodeKind::Text(StringStorage::Borrowed(content)),
            attrs: Box::new([]),
            children: vec![],
            static_hash: None,
        }
    }

    fn s(value: &'static str) -> SvgAttributeValue<'static> {
        SvgAttributeValue::from(value)
    }

    fn svg(children: Vec<NestedNodeData<'static>>) -> NestedSvgDocument<'static> {
        NestedSvgDocument::from_nodes(vec![Some(el(
            EId::Svg,
            vec![(AId::Width, s("200")), (AId::Height, s("100"))],
            children,
        ))])
    }

    fn written(doc: &NestedSvgDocument) -> String {
        let mut fontdb = usvgr::fontdb::Database::new();
        fontdb
            .load_font_data(include_bytes!("../../svgr/tests/fonts/NotoSans-Regular.ttf").to_vec());
        usvgr::Tree::from_nested_svgtree(doc, &usvgr::Options::default(), &fontdb)
            .unwrap()
            .to_string(&usvgr::WriteOptions {
                preserve_text: true,
                ..Default::default()
            })
    }

    /// Builds `doc` with the number written in the SVG and set from code, and checks both
    /// render the same, and that the written one shows `expected`.
    fn same_as_written(
        number: f32,
        written_as: &'static str,
        expected: &str,
        doc: impl Fn(SvgAttributeValue<'static>) -> NestedSvgDocument<'static>,
    ) {
        let from_svg = written(&doc(SvgAttributeValue::Float(
            number,
            StringStorage::Borrowed(written_as),
        )));
        let from_code = written(&doc(SvgAttributeValue::from(number)));
        assert!(
            from_svg.contains(expected),
            "the case doesn't show {expected}:\n{from_svg}"
        );
        assert_eq!(from_code, from_svg);
    }

    fn filtered(primitive: NestedNodeData<'static>) -> NestedSvgDocument<'static> {
        svg(vec![
            el(
                EId::Defs,
                vec![],
                vec![el(EId::Filter, vec![(AId::Id, s("f"))], vec![primitive])],
            ),
            el(
                EId::Rect,
                vec![
                    (AId::Width, s("50")),
                    (AId::Height, s("50")),
                    (AId::Fill, s("red")),
                    (AId::Filter, s("url(#f)")),
                ],
                vec![],
            ),
        ])
    }

    fn label(attr: AId, value: SvgAttributeValue<'static>) -> NestedSvgDocument<'static> {
        svg(vec![el(
            EId::Text,
            vec![
                (AId::X, s("10")),
                (AId::Y, s("60")),
                (AId::FontFamily, s("Noto Sans")),
                (AId::FontSize, s("40")),
                (attr, value),
            ],
            vec![text("Ab")],
        )])
    }

    #[test]
    fn font_weight() {
        same_as_written(700.0, "700", r#"font-weight="700""#, |v| {
            label(AId::FontWeight, v)
        });
    }

    /// A parent `<text>` with `parent` weight and a `<tspan>` with `child` weight.
    fn nested_weights(
        parent: SvgAttributeValue<'static>,
        child: SvgAttributeValue<'static>,
    ) -> NestedSvgDocument<'static> {
        svg(vec![el(
            EId::Text,
            vec![
                (AId::X, s("10")),
                (AId::Y, s("60")),
                (AId::FontFamily, s("Noto Sans")),
                (AId::FontSize, s("40")),
                (AId::FontWeight, parent),
            ],
            vec![el(
                EId::Tspan,
                vec![(AId::FontWeight, child)],
                vec![text("Ab")],
            )],
        )])
    }

    #[test]
    fn font_weight_not_a_number_keeps_the_inherited_weight() {
        for invalid in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            assert_eq!(
                written(&nested_weights(s("700"), SvgAttributeValue::from(invalid))),
                written(&nested_weights(s("700"), s("invalid"))),
            );
        }
    }

    #[test]
    fn font_weight_lighter_than_a_small_number() {
        // CSS Fonts 4: `lighter` keeps a weight below 100.
        let doc = nested_weights(SvgAttributeValue::from(1.0), s("lighter"));
        assert!(written(&doc).contains(r#"font-weight="1""#));
    }

    /// `bolder` and `lighter` follow the table of CSS Fonts 4, as browsers do (checked in
    /// Chrome 154), and keywords are case-insensitive.
    #[test]
    fn font_weight_relative_keywords() {
        let cases = [
            (1.0, "bolder", 400),
            (100.0, "bolder", 400),
            (300.0, "bolder", 400),
            (400.0, "bolder", 700),
            (500.0, "bolder", 700),
            (600.0, "bolder", 900),
            (650.0, "bolder", 900),
            (700.0, "bolder", 900),
            (800.0, "bolder", 900),
            (900.0, "bolder", 900),
            (1000.0, "bolder", 1000),
            (1.0, "lighter", 1),
            (100.0, "lighter", 100),
            (200.0, "lighter", 100),
            (400.0, "lighter", 100),
            (500.0, "lighter", 100),
            (600.0, "lighter", 400),
            (650.0, "lighter", 400),
            (700.0, "lighter", 400),
            (800.0, "lighter", 700),
            (900.0, "lighter", 700),
            (1000.0, "lighter", 700),
            (300.0, "BOLD", 700),
            (300.0, " Normal ", 400),
            (300.0, " 650 ", 650),
        ];
        for (parent, child, expected) in cases {
            let out = written(&nested_weights(SvgAttributeValue::from(parent), s(child)));
            // The writer leaves out the default weight.
            let weight = if expected == 400 {
                !out.contains("font-weight=")
            } else {
                out.contains(&format!(r#"font-weight="{expected}""#))
            };
            assert!(weight, "{parent} {child:?} should be {expected}:\n{out}");
        }
    }

    #[test]
    fn text_rotate() {
        same_as_written(20.0, "20", r#"rotate="20"#, |v| label(AId::Rotate, v));
    }

    #[test]
    fn marker_orient() {
        same_as_written(45.0, "45", "0.7071068", |v| {
            svg(vec![
                el(
                    EId::Defs,
                    vec![],
                    vec![el(
                        EId::Marker,
                        vec![
                            (AId::Id, s("m")),
                            (AId::MarkerWidth, s("10")),
                            (AId::MarkerHeight, s("10")),
                            (AId::Orient, v),
                        ],
                        vec![el(
                            EId::Path,
                            vec![(AId::D, s("M0 0 L10 5 L0 10z"))],
                            vec![],
                        )],
                    )],
                ),
                el(
                    EId::Path,
                    vec![
                        (AId::D, s("M10 50 L90 50")),
                        (AId::Stroke, s("black")),
                        (AId::MarkerEnd, s("url(#m)")),
                    ],
                    vec![],
                ),
            ])
        });
    }

    #[test]
    fn color_matrix_values() {
        same_as_written(45.0, "45", r#"values="45""#, |v| {
            filtered(el(
                EId::FeColorMatrix,
                vec![(AId::Type, s("hueRotate")), (AId::Values, v)],
                vec![],
            ))
        });
    }

    #[test]
    fn morphology_radius() {
        same_as_written(2.0, "2", r#"radius="2 2""#, |v| {
            filtered(el(
                EId::FeMorphology,
                vec![(AId::Operator, s("dilate")), (AId::Radius, v)],
                vec![],
            ))
        });
    }

    #[test]
    fn turbulence_base_frequency() {
        same_as_written(0.5, "0.5", r#"baseFrequency="0.5 0.5""#, |v| {
            filtered(el(EId::FeTurbulence, vec![(AId::BaseFrequency, v)], vec![]))
        });
    }

    #[test]
    fn transfer_table_values() {
        same_as_written(0.5, "0.5", r#"tableValues="0.5""#, |v| {
            filtered(el(
                EId::FeComponentTransfer,
                vec![],
                vec![el(
                    EId::FeFuncR,
                    vec![(AId::Type, s("discrete")), (AId::TableValues, v)],
                    vec![],
                )],
            ))
        });
    }

    /// Writes the tree twice: with text kept, and with text converted to paths, so that both
    /// the resolved attributes and their effect on the layout are compared.
    fn written_with_layout(doc: &NestedSvgDocument) -> String {
        let mut fontdb = usvgr::fontdb::Database::new();
        for font in [
            &include_bytes!("../../svgr/tests/fonts/NotoSans-Regular.ttf")[..],
            &include_bytes!("../../svgr/tests/fonts/NotoSans-Bold.ttf")[..],
            &include_bytes!("../../svgr/tests/fonts/NotoSans-Light.ttf")[..],
            &include_bytes!("../../svgr/tests/fonts/RobotoFlex.subset.ttf")[..],
        ] {
            fontdb.load_font_data(font.to_vec());
        }
        let tree =
            usvgr::Tree::from_nested_svgtree(doc, &usvgr::Options::default(), &fontdb).unwrap();
        let mut out = tree.to_string(&usvgr::WriteOptions {
            preserve_text: true,
            ..Default::default()
        });
        out.push_str(&tree.to_string(&usvgr::WriteOptions::default()));
        out
    }

    /// Builds the document of `template`, an SVG in which `?` marks the element under test.
    /// That element gets `attr` instead of the template's value of it, or none for `None`.
    fn from_template(
        template: &str,
        attr: Option<(AId, SvgAttributeValue<'static>)>,
    ) -> NestedSvgDocument<'static> {
        fn convert(
            node: usvgr::roxmltree::Node,
            attr: &Option<(AId, SvgAttributeValue<'static>)>,
        ) -> Option<NestedNodeData<'static>> {
            if node.is_text() {
                let content = node.text()?;
                return Some(NestedNodeData {
                    kind: NestedNodeKind::Text(StringStorage::Owned(content.into())),
                    attrs: Box::new([]),
                    children: vec![],
                    static_hash: None,
                });
            }
            let mut attrs: Attrs = node
                .attributes()
                .filter_map(|a| Some((AId::from_str(a.name())?, a.value().to_owned().into())))
                .collect();
            if node.has_attribute("under-test") {
                if let Some((aid, value)) = attr {
                    attrs.retain(|(name, _)| name != aid);
                    attrs.push((*aid, value.clone()));
                }
            }
            let children = node.children().filter_map(|c| convert(c, attr)).collect();
            Some(el(EId::from_str(node.tag_name().name())?, attrs, children))
        }

        let xml = template.replace('?', "under-test=''");
        let xml = usvgr::roxmltree::Document::parse(&xml).unwrap();
        NestedSvgDocument::from_nodes(vec![convert(xml.root_element(), &attr)])
    }

    macro_rules! doc {
        ($body:expr) => {
            concat!("<svg width='200' height='100'>", $body, "</svg>")
        };
    }

    macro_rules! filtered {
        ($primitive:literal) => {
            doc!(concat!(
                "<defs><filter id='f'>",
                $primitive,
                "</filter></defs><rect width='50' height='50' fill='red' filter='url(#f)'/>"
            ))
        };
    }

    macro_rules! painted {
        ($server:literal) => {
            doc!(concat!(
                "<defs>",
                $server,
                "</defs><rect width='100' height='100' fill='url(#p)'/>"
            ))
        };
    }

    const RECT: &str = doc!(
        "<rect ? x='10' y='10' width='60' height='40' fill='red' stroke='black' stroke-width='4'/>"
    );
    const ROTATED: &str =
        doc!("<rect ? x='10' y='10' width='60' height='40' transform='rotate(30)'/>");
    const STROKED: &str = doc!("<path ? d='M10 80 L50 10 L90 80' fill='none' stroke='black' stroke-width='10' stroke-dasharray='20 5'/>");
    const CIRCLE: &str = doc!("<circle ? cx='50' cy='50' r='20'/>");
    const ELLIPSE: &str = doc!("<ellipse ? cx='50' cy='50' rx='30' ry='20'/>");
    const LINE: &str = doc!("<line ? x1='10' y1='10' x2='90' y2='90' stroke='black'/>");
    const LINEAR: &str = painted!("<linearGradient ? id='p'><stop offset='0' stop-color='red'/><stop offset='1' stop-color='blue'/></linearGradient>");
    const RADIAL: &str = painted!("<radialGradient ? id='p'><stop offset='0' stop-color='red'/><stop offset='1' stop-color='blue'/></radialGradient>");
    const STOP: &str = painted!("<linearGradient id='p'><stop ? stop-color='red'/><stop offset='1' stop-color='blue'/></linearGradient>");
    const PATTERN: &str = painted!("<pattern ? id='p' patternUnits='userSpaceOnUse' width='20' height='20'><circle cx='10' cy='10' r='5'/></pattern>");
    const NESTED_SVG: &str = doc!(
        "<svg ? viewBox='0 0 10 10' width='50' height='50'><rect width='10' height='5'/></svg>"
    );
    const USE: &str = doc!("<defs><symbol id='s' viewBox='0 0 10 10'><rect width='10' height='5'/></symbol></defs><use ? href='#s' width='50' height='50'/>");
    const MARKER: &str = doc!("<defs><marker ? id='m' markerWidth='10' markerHeight='10' viewBox='0 0 10 10'><path d='M0 0 L10 5 L0 10z'/></marker></defs><path d='M10 50 L90 50' stroke='black' marker-end='url(#m)'/>");
    const MASK: &str = doc!("<defs><mask ? id='m' maskUnits='userSpaceOnUse'><rect width='100' height='100' fill='white'/></mask></defs><rect width='80' height='80' mask='url(#m)'/>");
    const LABEL: &str =
        doc!("<text ? x='10' y='60' font-family='Noto Sans' font-size='40'>Ab cd</text>");
    const VARIABLE_LABEL: &str =
        doc!("<text ? x='10' y='60' font-family='Roboto Flex' font-size='40'>Ab cd</text>");
    const KERNED_LABEL: &str =
        doc!("<text ? x='10' y='60' font-family='Noto Sans' font-size='40'>AVAT To</text>");
    const TSPAN: &str = doc!(
        "<text x='10' y='60' font-family='Noto Sans' font-size='40'>Ab<tspan ?>cd</tspan></text>"
    );
    const TEXT_PATH: &str = doc!("<defs><path id='t' d='M10 80 Q100 0 190 80'/></defs><text font-family='Noto Sans' font-size='20'><textPath ? href='#t'>Ab cd</textPath></text>");
    const FILTER_REGION: &str = doc!("<defs><filter ? id='f' filterUnits='userSpaceOnUse'><feGaussianBlur stdDeviation='2'/></filter></defs><rect width='50' height='50' filter='url(#f)'/>");
    const BLUR: &str = filtered!("<feGaussianBlur ?/>");
    const OFFSET: &str = filtered!("<feOffset ?/>");
    const FLOOD: &str = filtered!("<feFlood ? flood-color='green'/>");
    const DROP_SHADOW: &str = filtered!("<feDropShadow ?/>");
    const MORPHOLOGY: &str = filtered!("<feMorphology ? operator='dilate'/>");
    const TURBULENCE: &str = filtered!("<feTurbulence ? baseFrequency='0.05'/>");
    const SATURATE: &str = filtered!("<feColorMatrix ? type='saturate'/>");
    const LINEAR_TRANSFER: &str =
        filtered!("<feComponentTransfer><feFuncR ? type='linear'/></feComponentTransfer>");
    const GAMMA_TRANSFER: &str =
        filtered!("<feComponentTransfer><feFuncR ? type='gamma'/></feComponentTransfer>");
    const ARITHMETIC: &str = filtered!("<feComposite ? in2='SourceAlpha' operator='arithmetic' k1='0.1' k2='0.2' k3='0.3' k4='0.4'/>");
    const CONVOLVE: &str =
        filtered!("<feConvolveMatrix ? order='3' kernelMatrix='0 1 0 1 1 1 0 1 0'/>");
    const CONVOLVE_ONE: &str = filtered!("<feConvolveMatrix ? order='1'/>");
    const CONVOLVE_FOUR: &str = filtered!("<feConvolveMatrix ? kernelMatrix='1 0 0 1'/>");
    const DISPLACEMENT: &str =
        filtered!("<feDisplacementMap ? in2='SourceGraphic' xChannelSelector='R'/>");
    const DIFFUSE: &str = filtered!(
        "<feDiffuseLighting ? lighting-color='white'><feDistantLight/></feDiffuseLighting>"
    );
    const SPECULAR: &str = filtered!(
        "<feSpecularLighting ? lighting-color='white'><feDistantLight/></feSpecularLighting>"
    );
    const DISTANT_LIGHT: &str = filtered!(
        "<feDiffuseLighting lighting-color='white'><feDistantLight ?/></feDiffuseLighting>"
    );
    const POINT_LIGHT: &str = filtered!(
        "<feDiffuseLighting lighting-color='white'><fePointLight ?/></feDiffuseLighting>"
    );
    const SPOT_LIGHT: &str = filtered!("<feDiffuseLighting lighting-color='white'><feSpotLight ? limitingConeAngle='30'/></feDiffuseLighting>");
    const SPOT_LIGHT_CONE: &str =
        filtered!("<feDiffuseLighting lighting-color='white'><feSpotLight ?/></feDiffuseLighting>");

    /// Every attribute usvgr reads that accepts a plain number, in an element where it has an
    /// effect, with a number different from its default.
    const NUMBERS: &[(&str, f32, &str, &str)] = &[
        ("opacity", 0.5, "0.5", RECT),
        ("fill-opacity", 0.5, "0.5", RECT),
        ("stroke-opacity", 0.5, "0.5", RECT),
        ("stroke-width", 7.0, "7", RECT),
        ("x", 20.0, "20", RECT),
        ("y", 20.0, "20", RECT),
        ("width", 30.0, "30", RECT),
        ("height", 30.0, "30", RECT),
        ("rx", 5.0, "5", RECT),
        ("ry", 5.0, "5", RECT),
        ("transform-origin", 40.0, "40", ROTATED),
        ("stroke-miterlimit", 1.5, "1.5", STROKED),
        ("stroke-dashoffset", 3.0, "3", STROKED),
        ("stroke-dasharray", 4.0, "4", STROKED),
        ("cx", 40.0, "40", CIRCLE),
        ("cy", 40.0, "40", CIRCLE),
        ("r", 30.0, "30", CIRCLE),
        ("rx", 10.0, "10", ELLIPSE),
        ("ry", 10.0, "10", ELLIPSE),
        ("x1", 20.0, "20", LINE),
        ("y1", 20.0, "20", LINE),
        ("x2", 70.0, "70", LINE),
        ("y2", 70.0, "70", LINE),
        ("x1", 0.2, "0.2", LINEAR),
        ("y1", 0.2, "0.2", LINEAR),
        ("x2", 0.7, "0.7", LINEAR),
        ("y2", 0.7, "0.7", LINEAR),
        ("cx", 0.3, "0.3", RADIAL),
        ("cy", 0.3, "0.3", RADIAL),
        ("r", 0.3, "0.3", RADIAL),
        ("fx", 0.4, "0.4", RADIAL),
        ("fy", 0.4, "0.4", RADIAL),
        ("fr", 0.1, "0.1", RADIAL),
        ("offset", 0.3, "0.3", STOP),
        ("stop-opacity", 0.5, "0.5", STOP),
        ("x", 5.0, "5", PATTERN),
        ("y", 5.0, "5", PATTERN),
        ("width", 25.0, "25", PATTERN),
        ("height", 25.0, "25", PATTERN),
        ("x", 10.0, "10", NESTED_SVG),
        ("y", 10.0, "10", NESTED_SVG),
        ("width", 80.0, "80", NESTED_SVG),
        ("height", 80.0, "80", NESTED_SVG),
        ("x", 10.0, "10", USE),
        ("y", 10.0, "10", USE),
        ("width", 80.0, "80", USE),
        ("height", 80.0, "80", USE),
        ("markerWidth", 6.0, "6", MARKER),
        ("markerHeight", 6.0, "6", MARKER),
        ("refX", 2.0, "2", MARKER),
        ("refY", 2.0, "2", MARKER),
        ("orient", 45.0, "45", MARKER),
        ("x", 20.0, "20", MASK),
        ("y", 20.0, "20", MASK),
        ("width", 30.0, "30", MASK),
        ("height", 30.0, "30", MASK),
        ("font-size", 30.0, "30", LABEL),
        ("font-weight", 700.0, "700", LABEL),
        ("font-weight", 300.0, "300", LABEL),
        ("font-weight", 650.0, "650", LABEL),
        ("font-weight", 650.0, "650", VARIABLE_LABEL),
        ("font-weight", 550.5, "550.5", VARIABLE_LABEL),
        ("letter-spacing", 3.0, "3", LABEL),
        ("word-spacing", 9.0, "9", LABEL),
        ("x", 20.0, "20", LABEL),
        ("y", 70.0, "70", LABEL),
        ("dx", 4.0, "4", LABEL),
        ("dy", 4.0, "4", LABEL),
        ("rotate", 20.0, "20", LABEL),
        ("textLength", 150.0, "150", LABEL),
        ("kerning", 0.0, "0", KERNED_LABEL),
        ("baseline-shift", 6.0, "6", TSPAN),
        ("startOffset", 20.0, "20", TEXT_PATH),
        ("x", 5.0, "5", FILTER_REGION),
        ("y", 5.0, "5", FILTER_REGION),
        ("width", 40.0, "40", FILTER_REGION),
        ("height", 40.0, "40", FILTER_REGION),
        ("stdDeviation", 3.0, "3", BLUR),
        ("x", 5.0, "5", BLUR),
        ("y", 5.0, "5", BLUR),
        ("width", 20.0, "20", BLUR),
        ("height", 20.0, "20", BLUR),
        ("dx", 5.0, "5", OFFSET),
        ("dy", 5.0, "5", OFFSET),
        ("flood-opacity", 0.5, "0.5", FLOOD),
        ("dx", 5.0, "5", DROP_SHADOW),
        ("dy", 5.0, "5", DROP_SHADOW),
        ("stdDeviation", 3.0, "3", DROP_SHADOW),
        ("flood-opacity", 0.5, "0.5", DROP_SHADOW),
        ("radius", 2.0, "2", MORPHOLOGY),
        ("baseFrequency", 0.2, "0.2", TURBULENCE),
        ("numOctaves", 3.0, "3", TURBULENCE),
        ("seed", 7.0, "7", TURBULENCE),
        ("values", 0.5, "0.5", SATURATE),
        ("slope", 0.5, "0.5", LINEAR_TRANSFER),
        ("intercept", 0.5, "0.5", LINEAR_TRANSFER),
        ("amplitude", 0.5, "0.5", GAMMA_TRANSFER),
        ("exponent", 2.0, "2", GAMMA_TRANSFER),
        ("offset", 0.5, "0.5", GAMMA_TRANSFER),
        ("k1", 0.5, "0.5", ARITHMETIC),
        ("k2", 0.5, "0.5", ARITHMETIC),
        ("k3", 0.5, "0.5", ARITHMETIC),
        ("k4", 0.5, "0.5", ARITHMETIC),
        ("divisor", 2.0, "2", CONVOLVE),
        ("bias", 0.5, "0.5", CONVOLVE),
        ("targetX", 0.0, "0", CONVOLVE),
        ("targetY", 0.0, "0", CONVOLVE),
        ("kernelMatrix", 2.0, "2", CONVOLVE_ONE),
        ("order", 2.0, "2", CONVOLVE_FOUR),
        ("scale", 10.0, "10", DISPLACEMENT),
        ("surfaceScale", 3.0, "3", DIFFUSE),
        ("diffuseConstant", 2.0, "2", DIFFUSE),
        ("specularConstant", 2.0, "2", SPECULAR),
        ("specularExponent", 5.0, "5", SPECULAR),
        ("surfaceScale", 3.0, "3", SPECULAR),
        ("azimuth", 45.0, "45", DISTANT_LIGHT),
        ("elevation", 45.0, "45", DISTANT_LIGHT),
        ("x", 10.0, "10", POINT_LIGHT),
        ("y", 10.0, "10", POINT_LIGHT),
        ("z", 10.0, "10", POINT_LIGHT),
        ("x", 10.0, "10", SPOT_LIGHT),
        ("y", 10.0, "10", SPOT_LIGHT),
        ("z", 10.0, "10", SPOT_LIGHT),
        ("pointsAtX", 10.0, "10", SPOT_LIGHT),
        ("pointsAtY", 10.0, "10", SPOT_LIGHT),
        ("pointsAtZ", 10.0, "10", SPOT_LIGHT),
        ("specularExponent", 5.0, "5", SPOT_LIGHT),
        ("limitingConeAngle", 20.0, "20", SPOT_LIGHT_CONE),
    ];

    /// Every attribute usvgr reads that accepts a length with a unit. svgr-macro parses such a
    /// written value at compile time into `Length`, which has no text.
    const LENGTHS: &[(&str, &str, &str)] = &[
        ("opacity", "50%", RECT),
        ("fill-opacity", "50%", RECT),
        ("stroke-opacity", "50%", RECT),
        ("stroke-width", "7px", RECT),
        ("x", "20%", RECT),
        ("width", "30mm", RECT),
        ("rx", "5px", RECT),
        ("transform-origin", "40px", ROTATED),
        ("transform-origin", "50%", ROTATED),
        ("stroke-dashoffset", "3px", STROKED),
        ("stroke-dasharray", "4px", STROKED),
        ("cx", "40%", CIRCLE),
        ("r", "30px", CIRCLE),
        ("x2", "70%", LINE),
        ("x1", "20%", LINEAR),
        ("cx", "30%", RADIAL),
        ("fr", "10%", RADIAL),
        ("offset", "30%", STOP),
        ("stop-opacity", "50%", STOP),
        ("width", "25px", PATTERN),
        ("x", "10px", NESTED_SVG),
        ("width", "80%", NESTED_SVG),
        ("x", "10px", USE),
        ("markerWidth", "6px", MARKER),
        ("refX", "2px", MARKER),
        ("x", "10%", MASK),
        ("font-size", "150%", LABEL),
        ("font-size", "2em", LABEL),
        ("letter-spacing", "3px", LABEL),
        ("word-spacing", "0.5em", LABEL),
        ("x", "20px", LABEL),
        ("dx", "4px", LABEL),
        ("textLength", "150px", LABEL),
        ("kerning", "0px", KERNED_LABEL),
        ("baseline-shift", "20%", TSPAN),
        ("baseline-shift", "6px", TSPAN),
        ("startOffset", "20%", TEXT_PATH),
        ("x", "10%", FILTER_REGION),
        ("width", "5px", BLUR),
        ("flood-opacity", "50%", FLOOD),
    ];

    /// Checks that `text` written in the SVG has an effect, and that each of `parsed` renders
    /// the same. Returns what differs.
    fn same_as_text(
        name: &str,
        text: &'static str,
        template: &str,
        parsed: &[(&str, SvgAttributeValue<'static>)],
    ) -> Vec<String> {
        let aid = AId::from_str(name).unwrap();
        let case = format!("{name}={text} in {template}");
        let runtime = written_with_layout(&from_template(template, Some((aid, s(text)))));
        if runtime == written_with_layout(&from_template(template, None)) {
            return vec![format!("{case}: the written value has no effect")];
        }
        parsed
            .iter()
            .filter(|(_, value)| {
                written_with_layout(&from_template(template, Some((aid, value.clone())))) != runtime
            })
            .map(|(form, _)| format!("{case}: the {form} differs from the text"))
            .collect()
    }

    /// The same number renders the same whether it is written in the SVG (runtime parsing),
    /// parsed at compile time, or set from code.
    #[test]
    fn every_number_attribute() {
        let failures: Vec<_> = NUMBERS
            .iter()
            .flat_map(|&(name, number, text, template)| {
                same_as_text(
                    name,
                    text,
                    template,
                    &[
                        (
                            "compile-time number",
                            SvgAttributeValue::Float(number, StringStorage::Borrowed(text)),
                        ),
                        ("number from code", SvgAttributeValue::from(number)),
                    ],
                )
            })
            .collect();
        assert!(failures.is_empty(), "{}", failures.join("\n"));
    }

    /// A weight outside 1 to 1000 is invalid in CSS, written or set from code, and keeps the
    /// inherited weight.
    #[test]
    fn font_weight_out_of_range() {
        let inherited = written_with_layout(&from_template(LABEL, None));
        for (number, text) in [(1500.0, "1500"), (0.0, "0"), (-700.0, "-700"), (0.5, "0.5")] {
            for value in [s(text), SvgAttributeValue::from(number)] {
                let doc = from_template(LABEL, Some((AId::FontWeight, value)));
                assert_eq!(written_with_layout(&doc), inherited, "font-weight {text}");
            }
        }
    }

    /// A length written in the SVG renders the same as the one svgr-macro parses at compile
    /// time.
    #[test]
    fn every_length_attribute() {
        let failures: Vec<_> = LENGTHS
            .iter()
            .flat_map(|&(name, text, template)| {
                let length = text.parse().unwrap();
                same_as_text(
                    name,
                    text,
                    template,
                    &[("compile-time length", SvgAttributeValue::Length(length))],
                )
            })
            .collect();
        assert!(failures.is_empty(), "{}", failures.join("\n"));
    }
}
