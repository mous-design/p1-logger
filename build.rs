use std::path::Path;

/// `p1-web` embeds `frontend/dist` via `rust-embed` at compile time. Since a
/// single build script applies to the whole package (all three binaries),
/// not just `p1-web`, this can only warn, not hard-fail -- a `panic!` here
/// would also break `cargo build`/`--bin p1-logger` whenever the frontend
/// happens not to be built yet, which has nothing to do with p1-logger at
/// all. A p1-web built without the frontend still compiles (rust-embed's
/// `allow_missing`, see assets.rs) but refuses to start; this warning just
/// flags that at build time already.
fn main() {
    println!("cargo:rerun-if-changed=frontend/dist");
    if !Path::new("frontend/dist/index.html").exists() {
        println!("cargo:warning=frontend/dist/index.html not found -- run `./run build-frontend` before building p1-web");
    }
}
