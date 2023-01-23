use svgr::SvgrCache;
use usvgr_text_layout::{fontdb, FontsCache, TreeTextToPath, UsvgrTextLayoutCache};

fn main() {
    let opt = usvgr::Options::default();

    let mut fontdb = fontdb::Database::new();
    fontdb.load_system_fonts();

    let mut cache = SvgrCache::new(5);
    let mut text_layouts_cache = UsvgrTextLayoutCache::new(3);
    let mut fonts_cache = FontsCache::new();
    // This example shows how you can use cache to reuse rendering of inidividual nodes between rendering.
    // let mut cache = SvgrCache::none();

    let mut pixmap = tiny_skia::Pixmap::new(1000, 1000).unwrap();
    for i in 0..10 {
        pixmap.fill(tiny_skia::Color::from_rgba8(0, 0, 0, 0));

        let mut rtree = usvgr::Tree::from_str(
            &format!(
                r"<svg id='svg1' viewBox='0 0 1000 1000' xmlns='http://www.w3.org/2000/svg'>
  <filter id='blurMe'>
    <feGaussianBlur stdDeviation='25' />
  </filter>

  <rect id='rect1' x='2' y='0' filter='url(#blurMe)' width='500' height='500' fill='green' />
  <text x='0' y='700' font-size='50' fill='#fff'>render {i}</text>
  <text x='0' y='800' font-size='50' fill='#fff'>static text element</text>
  <text x='0' y='900' font-size='50' fill='#ababab'>second text element</text>
</svg>",
            ),
            &opt,
        )
        .unwrap();

        rtree.convert_text_with_cache(&fontdb, &mut text_layouts_cache, &mut fonts_cache, true);

        svgr::render(
            &rtree,
            usvgr::FitTo::Original,
            tiny_skia::Transform::default(),
            pixmap.as_mut(),
            &mut cache,
        )
        .unwrap();

        pixmap.save_png(format!("cache_{i}.png")).unwrap();
    }
}
