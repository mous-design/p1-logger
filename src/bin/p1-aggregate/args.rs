use std::path::PathBuf;

pub struct Args {
    pub data_dir: PathBuf,
    pub retention_days: Option<u32>,
    pub want_syslog: bool,
    pub quiet: bool,
}

pub fn parse_args(raw: &[String]) -> Result<Args, String> {
    let mut data_dir = None;
    let mut retention_days = None;
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
            "--retention-days" => {
                let value = iter.next().ok_or("--retention-days needs a value")?;
                retention_days =
                    Some(value.parse().map_err(|_| format!("--retention-days must be a non-negative integer, got {value:?}"))?);
            }
            other => return Err(format!("unrecognized argument: {other}")),
        }
    }
    let data_dir = data_dir.ok_or_else(|| "missing --data-dir".to_string())?;
    Ok(Args { data_dir, retention_days, want_syslog, quiet })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(raw: &[&str]) -> Result<Args, String> {
        parse_args(&raw.iter().map(|s| s.to_string()).collect::<Vec<_>>())
    }

    #[test]
    fn retention_days_is_optional() {
        let args = args(&["--data-dir", "/tmp/data"]).unwrap();
        assert_eq!(args.data_dir, PathBuf::from("/tmp/data"));
        assert_eq!(args.retention_days, None);
    }

    #[test]
    fn parses_retention_days_in_either_order() {
        let args = args(&["--retention-days", "90", "--data-dir", "/tmp/data"]).unwrap();
        assert_eq!(args.retention_days, Some(90));
    }

    #[test]
    fn rejects_non_numeric_retention_days() {
        assert_eq!(
            args(&["--data-dir", "/tmp/data", "--retention-days", "-1"]).err(),
            Some("--retention-days must be a non-negative integer, got \"-1\"".to_string())
        );
        assert_eq!(
            args(&["--data-dir", "/tmp/data", "--retention-days", "soon"]).err(),
            Some("--retention-days must be a non-negative integer, got \"soon\"".to_string())
        );
    }

    #[test]
    fn rejects_missing_data_dir() {
        assert_eq!(args(&["--retention-days", "90"]).err(), Some("missing --data-dir".to_string()));
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
