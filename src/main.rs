mod args;
mod db;
mod io_source;
mod parser;
mod telegram;

use std::env;
use std::io::BufReader;
use std::path::Path;
use std::process::ExitCode;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use syslog::{Facility, Formatter3164};

/// `std::sync::mpsc` has no `len()`, so the backlog is tracked by hand:
/// +1 right before a successful send, -1 once the writer thread has
/// finished a telegram. Only used to warn if the writer can't keep up
/// (e.g. a struggling SD card) -- Relaxed is fine, this is a monitoring
/// counter, not something anything is synchronized on.
const BACKLOG_WARN_THRESHOLD: usize = 200;

/// Warning/error sink that fans out to stderr and/or syslog independently.
/// stderr is on by default (never lose a warning/error silently) and only
/// `-q` turns it off; `-l` purely adds syslog on top, it never replaces
/// stderr. Info is allowed through too, but only ever used for the
/// start/stop lifecycle lines -- everything per-telegram stays off this
/// path entirely (see the `-d` record dump), so volume stays at "a couple
/// of lines per process lifetime", not "a couple of lines per second".
struct LogSinks {
    stderr: bool,
    syslog: Option<Mutex<syslog::Logger<syslog::LoggerBackend, Formatter3164>>>,
}

impl log::Log for LogSinks {
    fn enabled(&self, metadata: &log::Metadata) -> bool {
        metadata.level() <= log::Level::Info
    }

    fn log(&self, record: &log::Record) {
        if !self.enabled(record.metadata()) {
            return;
        }
        if self.stderr {
            eprintln!("{}: {}", record.level(), record.args());
        }
        if let Some(syslog) = &self.syslog {
            let message = record.args().to_string();
            let mut logger = syslog.lock().unwrap();
            let _ = match record.level() {
                log::Level::Error => logger.err(message),
                log::Level::Warn => logger.warning(message),
                _ => logger.info(message),
            };
        }
    }

    fn flush(&self) {}
}

fn init_logging(to_syslog: bool, to_stderr: bool) {
    let syslog = to_syslog.then(|| {
        let formatter = Formatter3164 {
            facility: Facility::LOG_DAEMON,
            hostname: None,
            process: "p1-logger".into(),
            pid: std::process::id(),
        };
        Mutex::new(syslog::unix(formatter).expect("failed to connect to syslog"))
    });

    log::set_boxed_logger(Box::new(LogSinks { stderr: to_stderr, syslog }))
        .map(|()| log::set_max_level(log::LevelFilter::Info))
        .expect("failed to install logger");
}

/// Writes the raw (lossily-decoded) text behind a noteworthy `TelegramEvent`
/// -- a telegram that failed to parse, or the stray lines from a
/// `HeaderLost`/`UnexplainedData` event -- for offline inspection of what
/// the noise actually looks like. `kind` names the event (used as the
/// filename prefix, e.g. "HeaderLost") so dumps sort/glob by what they are.
/// Opt-in via `--bad-telegram-dir`, and deliberately done inline on the
/// reader thread rather than through the channel: these events are rare
/// (per-hour, not per-second), so an occasional slow disk write here is
/// nowhere near the per-second stall this whole reader/writer split was
/// built to avoid.
fn dump_bad_telegram(dir: &Path, kind: &str, text: &str) {
    static COUNTER: AtomicUsize = AtomicUsize::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let path = dir.join(format!("{kind}-{now}-{n}.txt"));
    if let Err(err) = std::fs::write(&path, text) {
        log::error!("failed to write bad telegram dump to {path:?}: {err}");
    }
}

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

    // Normal operation: -l -q (syslog only). Testing: -d or no flags (stderr).
    let dump_records = args.debug && !args.quiet;
    init_logging(args.want_syslog, !args.quiet);
    log::info!("p1-logger starting: source={}, data_dir={:?}", args.source_path, args.data_dir);

    std::fs::create_dir_all(&args.data_dir).expect("failed to create data directory");
    if let Some(dir) = &args.bad_telegram_dir {
        std::fs::create_dir_all(dir).expect("failed to create bad-telegram-dir");
    }

    // The SQLite write (fsync, occasional SD-card latency spikes) must
    // never be able to stall the UART read: the tiny hardware FIFO and
    // the kernel's own serial buffer overflow silently if nobody reads
    // for too long, losing whole telegrams. So reading/framing/parsing
    // (this thread) and storing (the writer thread) are split across an
    // *unbounded* channel -- a bounded one would just move the same
    // stall-propagation problem to whatever its capacity is. Memory is
    // cheap relative to a telegram; a slow SD card should back up in RAM,
    // never on the wire.
    let (tx, rx) = mpsc::channel::<parser::Telegram>();
    let backlog = Arc::new(AtomicUsize::new(0));
    let writer_backlog = Arc::clone(&backlog);
    let mut store = db::RawStore::new(args.data_dir);
    let writer = std::thread::spawn(move || {
        for telegram in rx {
            if let Err(err) = store.insert(&telegram) {
                log::error!("failed to store telegram: {err}");
            }
            writer_backlog.fetch_sub(1, Ordering::Relaxed);
        }
    });

    let source =
        io_source::open_source(Path::new(&args.source_path)).expect("failed to open P1 source");
    let mut telegrams = telegram::TelegramReader::new(BufReader::new(source));

    while let Some(event) = telegrams.next_telegram().expect("failed to read from P1 source") {
        match event {
            telegram::TelegramEvent::Parsed { raw_text, result } => match result {
                Ok(telegram) => {
                    if dump_records {
                        for record in &telegram.records {
                            eprintln!("{}\t{}\t{}", record.timestamp, record.kind, record.value);
                        }
                    }
                    if tx.send(telegram).is_err() {
                        log::error!("writer thread has stopped; dropping telegram");
                    } else if backlog.fetch_add(1, Ordering::Relaxed) + 1 == BACKLOG_WARN_THRESHOLD {
                        log::warn!("telegram backlog reached {BACKLOG_WARN_THRESHOLD}; writer may be falling behind");
                    }
                }
                Err(err) => {
                    log::warn!("skipping bad telegram: {err}");
                    if let Some(dir) = &args.bad_telegram_dir {
                        dump_bad_telegram(dir, "bad-telegram", &raw_text);
                    }
                }
            },
            telegram::TelegramEvent::Resynced { lines } => {
                log::warn!("telegram resynced: abandoned {} lines mid-telegram", lines.len());
                if let Some(dir) = &args.bad_telegram_dir {
                    dump_bad_telegram(dir, "Resynced", &lines.concat());
                }
            }
            telegram::TelegramEvent::Truncated { lines } => {
                log::warn!("stream ended mid-telegram: abandoned {} lines", lines.len());
                if let Some(dir) = &args.bad_telegram_dir {
                    dump_bad_telegram(dir, "Truncated", &lines.concat());
                }
            }
            telegram::TelegramEvent::HeaderLost { lines } => {
                log::warn!("telegram header lost: {} lines still parsed via footer", lines.len());
                if let Some(dir) = &args.bad_telegram_dir {
                    dump_bad_telegram(dir, "HeaderLost", &lines.concat());
                }
            }
            telegram::TelegramEvent::UnexplainedData { lines } => {
                log::error!(
                    "unexplained data between a footer and the next header ({} lines) -- this should never happen: {lines:?}",
                    lines.len()
                );
                if let Some(dir) = &args.bad_telegram_dir {
                    dump_bad_telegram(dir, "UnexplainedData", &lines.concat());
                }
            }
        }
    }

    drop(tx);
    writer.join().expect("writer thread panicked");
    log::info!("p1-logger stopping: source exhausted (EOF)");
    ExitCode::SUCCESS
}
