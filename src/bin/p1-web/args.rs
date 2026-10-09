use std::path::PathBuf;

/// Default is `127.0.0.1` (never network-exposed by accident) -- the
/// systemd unit always passes an explicit `0.0.0.0:8080`, so this default
/// only ever matters for local `cargo run` testing, where "safe by
/// default" beats "convenient by default".
const DEFAULT_BIND_ADDR: &str = "127.0.0.1:8080";

pub struct Args {
    pub data_dir: PathBuf,
    pub bind_addr: String,
    pub want_syslog: bool,
    pub quiet: bool,
}

pub fn parse_args(raw: &[String]) -> Result<Args, String> {
    let mut data_dir = None;
    let mut bind_addr = None;
    let mut want_syslog = false;
    let mut quiet = false;
    let mut iter = raw.iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "-l" => want_syslog = true,
            "-q" => quiet = true,
            "--data-dir" => {
                let value = iter.next().ok_or("--data-dir needs a value")?;
                data_dir = Some(PathBuf::from(value));
            }
            "--bind" => {
                let value = iter.next().ok_or("--bind needs a value")?;
                bind_addr = Some(value.clone());
            }
            other => return Err(format!("unrecognized argument: {other}")),
        }
    }
    let data_dir = data_dir.ok_or_else(|| "missing --data-dir".to_string())?;
    Ok(Args { data_dir, bind_addr: bind_addr.unwrap_or_else(|| DEFAULT_BIND_ADDR.to_string()), want_syslog, quiet })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(raw: &[&str]) -> Result<Args, String> {
        parse_args(&raw.iter().map(|s| s.to_string()).collect::<Vec<_>>())
    }

    #[test]
    fn bind_addr_defaults_to_localhost() {
        let args = args(&["--data-dir", "/tmp/data"]).unwrap();
        assert_eq!(args.bind_addr, "127.0.0.1:8080");
    }

    #[test]
    fn parses_explicit_bind_addr() {
        let args = args(&["--data-dir", "/tmp/data", "--bind", "0.0.0.0:8080"]).unwrap();
        assert_eq!(args.bind_addr, "0.0.0.0:8080");
    }

    #[test]
    fn rejects_missing_data_dir() {
        assert_eq!(args(&["--bind", "0.0.0.0:8080"]).err(), Some("missing --data-dir".to_string()));
    }

    #[test]
    fn parses_syslog_and_quiet_flags() {
        let args = args(&["--data-dir", "/tmp/data", "-l", "-q"]).unwrap();
        assert!(args.want_syslog);
        assert!(args.quiet);
    }

    #[test]
    fn syslog_and_quiet_default_to_false() {
        let args = args(&["--data-dir", "/tmp/data"]).unwrap();
        assert!(!args.want_syslog);
        assert!(!args.quiet);
    }
}
