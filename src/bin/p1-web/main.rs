/// This is the binary that serves the React dashboard (embedded at compile
/// time, see `assets.rs`) and a JSON API reading `aggregates.sqlite3`
/// (read-only -- `p1-aggregate` is the only writer, see CLAUDE.md).
mod api;
mod args;
mod assets;
mod period;
mod queries;
mod run;

fn main() -> std::process::ExitCode {
    let raw_args: Vec<String> = std::env::args().skip(1).collect();
    let args = match args::parse_args(&raw_args) {
        Ok(args) => args,
        Err(err) => {
            eprintln!("error: {err}");
            eprintln!("usage: p1-web --data-dir <dir> [--bind <addr>] [-l] [-q]");
            return std::process::ExitCode::FAILURE;
        }
    };

    p1_logger::logging::init_logging("p1-web", args.want_syslog, !args.quiet);

    // Fail fast only on a genuine misconfiguration (data_dir missing) --
    // aggregates.sqlite3 itself not existing yet (p1-web started before
    // p1-aggregate's first cron run) is a normal startup ordering, not an
    // error, and this is a `Restart=always` daemon: crash-looping on that
    // would be worse than serving empty results until the file shows up.
    // So: no eagerly-opened connection here -- just the path. `queries.rs`
    // opens a short-lived connection per query, which naturally handles
    // "not created yet" as an empty result today and picks up real data
    // the moment the file appears, with no restart needed either way.
    if !args.data_dir.is_dir() {
        eprintln!("error: --data-dir {:?} does not exist", args.data_dir);
        return std::process::ExitCode::FAILURE;
    }

    if !assets::is_embedded() {
        eprintln!("error: p1-web was built without the dashboard -- run `./run build-frontend`, then rebuild");
        return std::process::ExitCode::FAILURE;
    }

    let aggregates_path = args.data_dir.join("aggregates.sqlite3");
    match run::run(&args.bind_addr, args.data_dir, aggregates_path) {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(err) => {
            log::error!("p1-web failed: {err}");
            std::process::ExitCode::FAILURE
        }
    }
}
