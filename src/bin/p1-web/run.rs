use std::path::{Path, PathBuf};
use std::sync::Arc;
use tiny_http::{Request, Response, Server};
use crate::{api, assets};

/// `tiny_http`'s `Server::recv()` blocks per-request on a single thread by
/// default; a page load fetches index.html + JS + CSS + at least one API
/// call, often more than one chart at once. A small fixed pool avoids
/// serializing those -- no async runtime needed for this, matching the
/// rest of this project's plain-thread style.
const WORKER_THREADS: usize = 4;

pub fn run(
    bind_addr: &str,
    data_dir: PathBuf,
    aggregates_path: PathBuf,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let server = Arc::new(Server::http(bind_addr)?);
    log::info!("p1-web listening on {bind_addr}");

    let data_dir = Arc::new(data_dir);
    let aggregates_path = Arc::new(aggregates_path);
    let workers: Vec<_> = (0..WORKER_THREADS)
        .map(|_| {
            let server = Arc::clone(&server);
            let data_dir = Arc::clone(&data_dir);
            let aggregates_path = Arc::clone(&aggregates_path);
            std::thread::spawn(move || loop {
                match server.recv() {
                    Ok(request) => handle_request(request, &data_dir, &aggregates_path),
                    Err(err) => log::error!("failed to receive request: {err}"),
                }
            })
        })
        .collect();

    for worker in workers {
        let _ = worker.join();
    }
    Ok(())
}

fn handle_request(request: Request, data_dir: &Path, aggregates_path: &Path) {
    let url = request.url().to_string();
    let (path, query) = url.split_once('?').unwrap_or((url.as_str(), ""));

    let response = api::handle(data_dir, aggregates_path, path, query)
        .or_else(|| assets::serve(path))
        .unwrap_or_else(|| Response::from_string("not found").with_status_code(404));
    if let Err(err) = request.respond(response) {
        log::warn!("failed to respond to request for {url}: {err}");
    }
}
