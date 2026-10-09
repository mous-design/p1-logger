use rust_embed::RustEmbed;
use tiny_http::{Header, Response};

/// The built React SPA (`frontend/dist`, produced by `./run build-frontend`)
/// embedded into the binary at compile time -- see `build.rs` for the
/// friendly warning if `dist/` doesn't exist yet. Keeps deployment to
/// "one binary, one scp", matching `p1-logger`/`p1-aggregate`.
///
/// `allow_missing`: `frontend/dist` is gitignored build output, so a fresh
/// clone doesn't have it -- and without this, rust-embed refuses to compile,
/// which (one package, one build) would block `cargo build`/`cargo test` for
/// p1-logger and p1-aggregate too. With it, p1-web compiles with nothing
/// embedded instead, and refuses to start on that (see `is_embedded`).
#[derive(RustEmbed)]
#[folder = "frontend/dist"]
#[allow_missing = true]
struct Assets;

/// `false` if p1-web was built without a frontend (see `allow_missing`
/// above) -- `main` treats that as a startup misconfiguration.
pub fn is_embedded() -> bool {
    Assets::get("index.html").is_some()
}

/// Small, explicit extension -> Content-Type mapping instead of relying on
/// a mime-guessing crate feature: the set of file types a Vite build ever
/// produces is small and fixed, so a manual list stays simple and never
/// surprises.
fn content_type(path: &str) -> &'static str {
    match path.rsplit('.').next() {
        Some("html") => "text/html; charset=utf-8",
        Some("js") => "text/javascript; charset=utf-8",
        Some("css") => "text/css; charset=utf-8",
        Some("svg") => "image/svg+xml",
        Some("json") => "application/json",
        Some("ico") => "image/x-icon",
        Some("png") => "image/png",
        Some("woff2") => "font/woff2",
        _ => "application/octet-stream",
    }
}

fn respond(asset_path: &str) -> Option<Response<std::io::Cursor<Vec<u8>>>> {
    let file = Assets::get(asset_path)?;
    let header = Header::from_bytes(&b"Content-Type"[..], content_type(asset_path).as_bytes())
        .expect("static header name/value are always valid");
    let response = Response::from_data(file.data.into_owned()).with_header(header);
    // Vite puts a content hash in every filename under assets/, so a given
    // URL there never changes content -- safe to cache forever. Everything
    // else (index.html, fonts/, favicon) keeps a stable name across builds
    // and must not be cached that way, or a redeploy would go unseen.
    if !asset_path.starts_with("assets/") {
        return Some(response);
    }
    let cache = Header::from_bytes(&b"Cache-Control"[..], &b"public, max-age=31536000, immutable"[..])
        .expect("static header name/value are always valid");
    Some(response.with_header(cache))
}

/// Serves `index.html` at `/`, the embedded asset matching the request path
/// exactly (e.g. `/assets/index-XYZ.js`), or -- for anything else that
/// doesn't look like a file request -- `index.html` again, so the SPA's own
/// client-side router can take over. That fallback is what makes a deep
/// link like `/detail/power` survive a hard reload: the server has no
/// route for it, but the last path segment has no extension, so it isn't
/// mistaken for a missing asset either. A genuinely missing asset (a
/// typo'd `/assets/foo.js`) still 404s, since that segment *does* have a
/// dot in it.
pub fn serve(path: &str) -> Option<Response<std::io::Cursor<Vec<u8>>>> {
    let asset_path = if path == "/" { "index.html" } else { path.trim_start_matches('/') };
    if let Some(response) = respond(asset_path) {
        return Some(response);
    }
    let last_segment = path.rsplit('/').next().unwrap_or("");
    if !last_segment.contains('.') {
        return respond("index.html");
    }
    None
}
