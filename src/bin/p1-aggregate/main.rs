/// This is the binary that is involved with reading the sqlite db's raw table,
/// aggregates that day to 1 minute and day-total for power and hour-records for
/// Wh-counter and gas-counter.
mod args;
mod run;

use p1_logger::{aggregate, logging};
use std::process::ExitCode;

fn main() -> ExitCode {
    let raw_args: Vec<String> = std::env::args().skip(1).collect();
    let args = match args::parse_args(&raw_args) {
        Ok(args) => args,
        Err(err) => {
            eprintln!("error: {err}");
            eprintln!("usage: p1-aggregate --data-dir <dir> [--retention-days <n>] [-l] [-q]");
            return ExitCode::FAILURE;
        }
    };

    logging::init_logging("p1-aggregate", args.want_syslog, !args.quiet);

    let aggregates_path = args.data_dir.join("aggregates.sqlite3");
    let aggregates_conn = aggregate::open_aggregates_db(&aggregates_path).expect("failed to open aggregates database");

    run::run(args.data_dir, args.retention_days, aggregates_conn)
}
