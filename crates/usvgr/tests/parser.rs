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
fn plain_shapes_match_temporary_group_conversion() {
    let fontdb = usvgr::fontdb::Database::new();
    let shapes = [
        "<rect EXTRA x='10%' y='5' width='40%' height='30' rx='4'/>",
        "<circle EXTRA cx='40' cy='40' r='20'/>",
        "<ellipse EXTRA cx='40' cy='40' rx='20' ry='10'/>",
        "<line EXTRA x1='10' y1='10' x2='60' y2='40'/>",
        "<polyline EXTRA points='10,10 40,60 70,10'/>",
        "<polygon EXTRA points='10,10 40,60 70,10'/>",
        "<path EXTRA d='M10 10 Q40 60 70 10 L70 60 Z'/>",
    ];
    for parent in [
        "fill='currentColor' color='red' stroke='blue' stroke-width='3'",
        "transform='translate(.25 .5) rotate(20 50 50)' fill='url(#gradient)' opacity='.6'",
        "clip-path='url(#clip)' mask='url(#mask)' filter='url(#blur)'",
        "marker-start='url(#marker)' marker-end='url(#marker)' stroke='blue' paint-order='stroke markers fill'",
    ] {
        for shape in shapes {
            let convert = |extra: &str| {
                let shape = shape.replace("EXTRA", extra);
                let svg = format!(
                    "<svg xmlns='http://www.w3.org/2000/svg' width='100' height='100'>
                    <defs>
                        <linearGradient id='gradient'><stop stop-color='red'/><stop offset='1' stop-color='blue'/></linearGradient>
                        <clipPath id='clip'><circle cx='50' cy='50' r='40'/></clipPath>
                        <mask id='mask'><rect width='100' height='100' fill='white'/></mask>
                        <filter id='blur'><feGaussianBlur stdDeviation='2'/></filter>
                        <marker id='marker' markerWidth='6' markerHeight='6'><path d='M0 0 L6 3 L0 6 Z'/></marker>
                    </defs><g {parent}>{shape}</g></svg>"
                );
                usvgr::Tree::from_str(&svg, &usvgr::Options::default(), &fontdb)
                    .unwrap()
                    .to_string(&usvgr::WriteOptions::default())
            };
            // An explicit no-op filter keeps the original temporary-group path.
            assert_eq!(convert(""), convert("filter='none'"), "{parent}: {shape}");
        }
    }
}
