use std::path::PathBuf;

pub const USAGE: &str =
    "usage: p1-logger <serial-device-or-file> [--data-dir <dir>] [--bad-telegram-dir <dir>] [-d] [-l] [-q]";

#[derive(Debug, PartialEq)]
pub struct Args {
    pub source_path: String,
    pub data_dir: PathBuf,
    pub bad_telegram_dir: Option<PathBuf>,
    pub debug: bool,
    pub want_syslog: bool,
    pub quiet: bool,
}

/// Every token must be consumed as either the one positional argument, a
/// known flag, or a known flag's value -- anything left over is a hard
/// error rather than being silently ignored
pub fn parse_args(raw: &[String]) -> Result<Args, String> {
    let mut source_path = None;
    let mut data_dir = None;
    let mut bad_telegram_dir = None;
    let mut debug = false;
    let mut want_syslog = false;
    let mut quiet = false;

    let mut iter = raw.iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "-d" => debug = true,
            "-l" => want_syslog = true,
            "-q" => quiet = true,
            "--data-dir" => {
                let value = iter.next().ok_or("--data-dir needs a value")?;
                data_dir = Some(PathBuf::from(value));
            }
            "--bad-telegram-dir" => {
                let value = iter.next().ok_or("--bad-telegram-dir needs a value")?;
                bad_telegram_dir = Some(PathBuf::from(value));
            }
            _ if source_path.is_none() => source_path = Some(arg.clone()),
            other => return Err(format!("unrecognized argument: {other}")),
        }
    }

    Ok(Args {
        source_path: source_path.ok_or("missing <serial-device-or-file>")?,
        data_dir: data_dir.unwrap_or_else(|| PathBuf::from("data")),
        bad_telegram_dir,
        debug,
        want_syslog,
        quiet,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(raw: &[&str]) -> Result<Args, String> {
        parse_args(&raw.iter().map(|s| s.to_string()).collect::<Vec<_>>())
    }

    #[test]
    fn parses_source_and_flags_in_any_order() {
        let parsed = args(&["-d", "/dev/serial0", "--data-dir", "/tmp/data", "-l"]).unwrap();
        assert_eq!(parsed.source_path, "/dev/serial0");
        assert_eq!(parsed.data_dir, PathBuf::from("/tmp/data"));
        assert!(parsed.debug);
        assert!(parsed.want_syslog);
        assert!(!parsed.quiet);
        assert_eq!(parsed.bad_telegram_dir, None);
    }

    #[test]
    fn rejects_unrecognized_flag() {
        // The exact bug that prompted this: a typo'd single dash.
        assert_eq!(
            args(&["/dev/serial0", "-bad-telegram-dir", "/tmp/x"]),
            Err("unrecognized argument: -bad-telegram-dir".to_string())
        );
    }

    #[test]
    fn rejects_missing_source_path() {
        assert_eq!(args(&["-d"]), Err("missing <serial-device-or-file>".to_string()));
    }

    #[test]
    fn rejects_flag_missing_its_value() {
        assert_eq!(
            args(&["/dev/serial0", "--data-dir"]),
            Err("--data-dir needs a value".to_string())
        );
    }

    #[test]
    fn rejects_second_positional_argument() {
        assert_eq!(
            args(&["/dev/serial0", "extra.txt"]),
            Err("unrecognized argument: extra.txt".to_string())
        );
    }
}
