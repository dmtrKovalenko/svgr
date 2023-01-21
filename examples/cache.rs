use std::num::NonZeroUsize;
use svgr::SvgrCache;

fn main() {
    let mut opt = usvgr::Options::default();
    // opt.fontdb.load_system_fonts();

    let mut cache = SvgrCache::new(NonZeroUsize::new(5).unwrap());
    // This example shows how you can use cache to reuse rendering of inidividual nodes between rendering.
    // let mut cache = SvgrCache::none();

    let mut pixmap = tiny_skia::Pixmap::new(1000, 1000).unwrap();
    for i in 0..10 {
        pixmap.fill(
            tiny_skia::Color::from_rgba8(0, 0, 0, 0),
        );
       
        let rtree = usvgr::Tree::from_str(
            &format!(
                r"<svg id='svg1' viewBox='0 0 1000 1000' xmlns='http://www.w3.org/2000/svg'>
  <filter id='blurMe'>
    <feGaussianBlur stdDeviation='25' />
  </filter>

  <rect id='rect1' x='2' y='0' filter='url(#blurMe)' width='500' height='500' fill='green' />
  <text x='0' y='700' font-size='50' fill='#fff'>render #{}</text>
</svg>",
                i
            ),
            &opt,
        )
        .unwrap();

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
