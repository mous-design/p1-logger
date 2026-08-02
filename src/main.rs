mod run;

use p1_logger::{args, db, io_source, logging, telegram};
use std::env;
use std::io::BufReader;
use std::path::Path;
use std::process::ExitCode;

fn main() -> ExitCode {
    let raw_args: Vec<String> = env::args().skip(1).collect();
    let args = match args::parse_args(&raw_args) {
        Ok(args) => args,
        Err(err) => {
            eprintln!("error: {err}");
            eprintln!("{}", args::USAGE);
            return ExitCode::FAILURE;
        }
    };

    logging::init_logging("p1-logger", args.want_syslog, !args.quiet);
    log::info!("p1-logger starting: source={}, data_dir={:?}", args.source_path, args.data_dir);

    std::fs::create_dir_all(&args.data_dir).expect("failed to create data directory");
    if let Some(dir) = &args.bad_telegram_dir {
        std::fs::create_dir_all(dir).expect("failed to create bad-telegram-dir");
    }

    // Opening the P1 source can fail immediately (bad device path,
    // permissions) -- exactly the kind of startup-time problem that should
    // crash loudly here, rather than be buried inside the run loop.
    let source = io_source::open_source(Path::new(&args.source_path)).expect("failed to open P1 source");
    let telegrams = telegram::TelegramReader::new(BufReader::new(source));
    let store = db::RawStore::new(args.data_dir);

    // Normal operation: -l -q (syslog only). Testing: -d or no flags (stderr).
    let dump_records = args.debug && !args.quiet;
    run::run(telegrams, store, args.bad_telegram_dir, dump_records)
}
