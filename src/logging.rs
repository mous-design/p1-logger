use std::sync::Mutex;
use syslog::{Facility, Formatter3164};

/// Warning/error sink that fans out to stderr and/or syslog independently.
/// Shared by both binaries so a `log::warn!`/`log::error!` call always ends
/// up somewhere -- never a silent no-op because nobody installed a logger.
pub struct LogSinks {
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

pub fn init_logging(process_name: &'static str, to_syslog: bool, to_stderr: bool) {
    let syslog = to_syslog.then(|| {
        let formatter = Formatter3164 {
            facility: Facility::LOG_DAEMON,
            hostname: None,
            process: process_name.into(),
            pid: std::process::id(),
        };
        Mutex::new(syslog::unix(formatter).expect("failed to connect to syslog"))
    });

    log::set_boxed_logger(Box::new(LogSinks { stderr: to_stderr, syslog }))
        .map(|()| log::set_max_level(log::LevelFilter::Info))
        .expect("failed to install logger");
}
