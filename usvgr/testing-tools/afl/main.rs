extern crate afl;
extern crate usvgr;

use std::str;

use afl::fuzz;

fn main() {
    let opt = usvgr::Options::default();

    fuzz(|data| {
        if let Ok(text) = str::from_utf8(data) {
            let _ = usvgr::Tree::from_str(text, &opt);
        }
    });
}
