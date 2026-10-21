//! Compresses the PDF export's bundled fonts (`assets/`) into `OUT_DIR`, so
//! the binary carries them deflated; `src/pdf/font.rs` inflates each one the
//! first time a PDF needs it.

use std::io::Write as _;
use std::path::Path;

/// The bundled fonts, relative to `assets/`. Keep in step with `BUNDLED` in
/// `src/pdf/font.rs`.
const FONTS: &[&str] = &[
    "DejaVuSans.ttf",
    "noto/NotoSansArabic-Regular.ttf",
    "noto/NotoSansBengali-Regular.ttf",
    "noto/NotoSansDevanagari-Regular.ttf",
    "noto/NotoSansGujarati-Regular.ttf",
    "noto/NotoSansGurmukhi-Regular.ttf",
    "noto/NotoSansKannada-Regular.ttf",
    "noto/NotoSansMalayalam-Regular.ttf",
    "noto/NotoSansOriya-Regular.ttf",
    "noto/NotoSansTamil-Regular.ttf",
    "noto/NotoSansTelugu-Regular.ttf",
];

fn main() {
    let out = std::env::var("OUT_DIR").expect("OUT_DIR");
    for f in FONTS {
        let src = Path::new("assets").join(f);
        println!("cargo:rerun-if-changed={}", src.display());
        let data = std::fs::read(&src).unwrap_or_else(|e| panic!("{}: {e}", src.display()));
        let mut z = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::best());
        z.write_all(&data).expect("writing to memory");
        let name = Path::new(f).file_name().expect("a file name").to_string_lossy().into_owned();
        std::fs::write(Path::new(&out).join(format!("{name}.z")), z.finish().expect("writing to memory")).expect("writing OUT_DIR");
    }
    println!("cargo:rerun-if-changed=build.rs");
}
