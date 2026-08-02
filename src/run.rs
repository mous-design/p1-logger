use p1_logger::db::RawStore;
use p1_logger::parser;
use p1_logger::telegram::{TelegramEvent, TelegramReader};
use std::io::{BufReader, Read};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{mpsc, Arc};

/// `std::sync::mpsc` has no `len()`, so the backlog is tracked by hand:
/// +1 right before a successful send, -1 once the writer thread has
/// finished a telegram. Only used to warn if the writer can't keep up
/// (e.g. a struggling SD card) -- Relaxed is fine, this is a monitoring
/// counter, not something anything is synchronized on.
const BACKLOG_WARN_THRESHOLD: usize = 200;

/// Drives the read-parse-store pipeline until the source is exhausted
/// (EOF): spawns a writer thread fed from `store`, then reads and dispatches
/// every `TelegramEvent` from `telegrams` until there's nothing left.
/// `telegrams`/`store` are already-opened/constructed by `main()` -- opening
/// the P1 source is exactly the kind of startup-time problem that should
/// crash loudly there, not be buried inside the run loop.
pub fn run(
    mut telegrams: TelegramReader<BufReader<Box<dyn Read>>>,
    mut store: RawStore,
    bad_telegram_dir: Option<PathBuf>,
    dump_records: bool,
) -> ExitCode {
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
    let writer = std::thread::spawn(move || {
        for telegram in rx {
            if let Err(err) = store.insert(&telegram) {
                log::error!("failed to store telegram: {err}");
            }
            writer_backlog.fetch_sub(1, Ordering::Relaxed);
        }
    });

    while let Some(event) = telegrams.next_telegram().expect("failed to read from P1 source") {
        match event {
            TelegramEvent::Parsed { raw_text, result } => match result {
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
                    if let Some(dir) = &bad_telegram_dir {
                        dump_bad_telegram(dir, "bad-telegram", &raw_text);
                    }
                }
            },
            TelegramEvent::Resynced { lines } => {
                log::warn!("telegram resynced: abandoned {} lines mid-telegram", lines.len());
                if let Some(dir) = &bad_telegram_dir {
                    dump_bad_telegram(dir, "Resynced", &lines.concat());
                }
            }
            TelegramEvent::Truncated { lines } => {
                log::warn!("stream ended mid-telegram: abandoned {} lines", lines.len());
                if let Some(dir) = &bad_telegram_dir {
                    dump_bad_telegram(dir, "Truncated", &lines.concat());
                }
            }
            TelegramEvent::HeaderLost { lines } => {
                log::warn!("telegram header lost: {} lines still parsed via footer", lines.len());
                if let Some(dir) = &bad_telegram_dir {
                    dump_bad_telegram(dir, "HeaderLost", &lines.concat());
                }
            }
            TelegramEvent::UnexplainedData { lines } => {
                log::error!(
                    "unexplained data between a footer and the next header ({} lines) -- this should never happen: {lines:?}",
                    lines.len()
                );
                if let Some(dir) = &bad_telegram_dir {
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
