use std::path::Path;

fn main() {
    // `tauri::generate_context!` embeds the Vite bundle from `../dist`. A
    // workspace-wide `cargo test` or `cargo build` must not need a Node build
    // first, so write a placeholder page when the bundle is missing.
    // `pnpm build` replaces it with the real frontend.
    let dist = Path::new(env!("CARGO_MANIFEST_DIR")).join("../dist");
    if !dist.join("index.html").exists() {
        std::fs::create_dir_all(&dist).expect("create dist");
        std::fs::write(
            dist.join("index.html"),
            "<!doctype html><meta charset=\"utf-8\"><title>KhataOS</title>\
             <p style=\"font-family:system-ui;padding:2rem\">Frontend not built. \
             Run <code>pnpm install &amp;&amp; pnpm build</code> in apps/desktop, then rebuild.</p>",
        )
        .expect("write placeholder");
    }
    println!("cargo:rerun-if-changed=../dist/index.html");
    tauri_build::build()
}
