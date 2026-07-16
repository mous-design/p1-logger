use chrono::{FixedOffset, NaiveDate, TimeZone};
use std::fmt;

#[derive(Debug, PartialEq)]
pub struct Record {
    pub kind: &'static str,
    pub timestamp: i64,
    pub value: i64,
}

#[derive(Debug, PartialEq)]
pub struct Telegram {
    /// The telegram's own `0-0:1.0.0` timestamp. Exposed explicitly (not
    /// inferred from `records[0]`) because OBIS-code order within a
    /// telegram is not spec-guaranteed -- callers that need to bucket a
    /// whole telegram (e.g. by calendar day) should use this, not a record.
    pub timestamp: i64,
    pub records: Vec<Record>,
}

#[derive(Debug, PartialEq)]
pub enum ParseError {
    CrcMissing,
    CrcMismatch { expected: u16, actual: u16 },
    MissingTimestamp,
    InvalidTimestamp(String),
    InvalidValue(String),
    MalformedLine(String),
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ParseError::CrcMissing => write!(f, "telegram has no valid CRC footer"),
            ParseError::CrcMismatch { expected, actual } => {
                write!(f, "CRC mismatch: expected {expected:04X}, got {actual:04X}")
            }
            ParseError::MissingTimestamp => write!(f, "telegram has no 0-0:1.0.0 timestamp"),
            ParseError::InvalidTimestamp(s) => write!(f, "invalid timestamp: {s:?}"),
            ParseError::InvalidValue(s) => write!(f, "invalid value: {s:?}"),
            ParseError::MalformedLine(s) => write!(f, "malformed line, not back-to-back (...) groups: {s:?}"),
        }
    }
}

impl std::error::Error for ParseError {}

/// Parses one DSMR5 telegram (CRC-checked, `\r\n`-delimited) into its
/// interesting records. Two passes over the text, per the P1 Companion
/// Standard: pass 1 finds the telegram timestamp (OBIS-code order is not
/// guaranteed), pass 2 turns each recognised line into a `Record`, using
/// the telegram timestamp unless the line carries its own (e.g. the gas
/// meter reading, relayed through wireless M-Bus with a delayed stamp).
pub fn parse_telegram(raw: &str) -> Result<Telegram, ParseError> {
    verify_crc(raw)?;
    let timestamp = find_timestamp(raw)?;
    let records = parse_records(raw, timestamp)?;
    Ok(Telegram { timestamp, records })
}

fn verify_crc(raw: &str) -> Result<(), ParseError> {
    let bang_pos = raw.find('!').ok_or(ParseError::CrcMissing)?;
    let (body, after_bang) = raw.split_at(bang_pos + 1);
    let hex = after_bang
        .trim_end()
        .get(0..4)
        .ok_or(ParseError::CrcMissing)?;
    let expected = u16::from_str_radix(hex, 16).map_err(|_| ParseError::CrcMissing)?;
    let actual = crc16_arc(body.as_bytes());

    if actual == expected {
        Ok(())
    } else {
        Err(ParseError::CrcMismatch { expected, actual })
    }
}

fn crc16_arc(data: &[u8]) -> u16 {
    let mut crc: u16 = 0x0000;
    for &b in data {
        crc ^= b as u16;
        for _ in 0..8 {
            crc = if crc & 1 != 0 {
                (crc >> 1) ^ 0xA001
            } else {
                crc >> 1
            };
        }
    }
    crc
}

fn find_timestamp(raw: &str) -> Result<i64, ParseError> {
    raw.lines()
        .find_map(|line| line.strip_prefix("0-0:1.0.0("))
        .map(|inner| parse_dsmr_timestamp(inner.trim_end_matches(')')))
        .ok_or(ParseError::MissingTimestamp)?
}

/// Standard (winter) UTC offset for the Netherlands, CET. DSMR is a Dutch
/// standard, also used as-is by Belgium and Luxembourg, which sit in the
/// same CET/CEST zone under the same EU DST rule -- so the S/W flag below
/// never needs to carry more than a summer-time delta on top of this. If
/// this parser is ever reused for a P1-alike protocol outside the
/// Benelux, this is the constant to change.
const STANDARD_UTC_OFFSET_HOURS: i32 = 1;

/// `YYMMDDhhmmssX`, where X = `S` (summer/DST active) or `W` (winter/
/// standard time) is the offset itself, not a hint to resolve against a
/// timezone database.
fn parse_dsmr_timestamp(raw: &str) -> Result<i64, ParseError> {
    let bad = || ParseError::InvalidTimestamp(raw.to_string());

    if raw.len() != 13 {
        return Err(bad());
    }
    let (digits, flag) = raw.split_at(12);
    let dst_offset_hours = match flag {
        "S" => 1,
        "W" => 0,
        _ => return Err(bad()),
    };
    let offset_hours = STANDARD_UTC_OFFSET_HOURS + dst_offset_hours;

    let field = |range: std::ops::Range<usize>| digits[range].parse::<u32>().map_err(|_| bad());
    let year = 2000 + field(0..2)? as i32;
    let (month, day) = (field(2..4)?, field(4..6)?);
    let (hour, minute, second) = (field(6..8)?, field(8..10)?, field(10..12)?);

    let date = NaiveDate::from_ymd_opt(year, month, day).ok_or_else(bad)?;
    let naive = date.and_hms_opt(hour, minute, second).ok_or_else(bad)?;
    let offset = FixedOffset::east_opt(offset_hours * 3600).ok_or_else(bad)?;

    offset
        .from_local_datetime(&naive)
        .single()
        .map(|dt| dt.timestamp())
        .ok_or_else(bad)
}

fn obis_kind(code: &str) -> Option<&'static str> {
    match code {
        "1-0:1.8.1" => Some("E-import-T1"),
        "1-0:1.8.2" => Some("E-import-T2"),
        "1-0:2.8.1" => Some("E-export-T1"),
        "1-0:2.8.2" => Some("E-export-T2"),
        "1-0:1.7.0" => Some("P-import"),
        "1-0:2.7.0" => Some("P-export"),
        "1-0:21.7.0" => Some("P-L1-import"),
        "1-0:41.7.0" => Some("P-L2-import"),
        "1-0:61.7.0" => Some("P-L3-import"),
        "1-0:22.7.0" => Some("P-L1-export"),
        "1-0:42.7.0" => Some("P-L2-export"),
        "1-0:62.7.0" => Some("P-L3-export"),
        "0-1:24.2.1" => Some("G-import"),
        _ => None,
    }
}

fn parse_records(raw: &str, telegram_timestamp: i64) -> Result<Vec<Record>, ParseError> {
    let mut records = Vec::new();

    for line in raw.lines() {
        let Some(paren) = line.find('(') else {
            continue;
        };
        let Some(kind) = obis_kind(&line[..paren]) else {
            continue;
        };

        let groups = extract_groups(&line[paren..])?;
        let (timestamp, value_raw) = match groups.as_slice() {
            [value] => (telegram_timestamp, *value),
            [own_timestamp, value] => (parse_dsmr_timestamp(own_timestamp)?, *value),
            _ => continue,
        };

        if value_raw.is_empty() {
            continue;
        }

        let numeric = value_raw.split('*').next().unwrap();
        records.push(Record {
            kind,
            timestamp,
            value: parse_scaled_int(numeric)?,
        });
    }

    Ok(records)
}

/// Extracts the contents of each `(...)` group in order. Groups must be
/// back-to-back with nothing between them, e.g. `(a)(b)` -- anything else
/// (stray characters between groups, an unclosed group) is a malformed
/// line rather than something to silently skip over.
fn extract_groups(s: &str) -> Result<Vec<&str>, ParseError> {
    let bad = || ParseError::MalformedLine(s.to_string());
    let mut groups = Vec::new();
    let mut rest = s;
    while !rest.is_empty() {
        if !rest.starts_with('(') {
            return Err(bad());
        }
        let close = rest.find(')').ok_or_else(bad)?;
        groups.push(&rest[1..close]);
        rest = &rest[close + 1..];
    }
    Ok(groups)
}

/// `"005081.531"` -> `5_081_531`. Converts by string concatenation instead
/// of float math to avoid rounding: kWh -> Wh and m3 -> dm3 both happen to
/// be a fixed x1000 shift, i.e. "keep 3 fraction digits".
fn parse_scaled_int(numeric: &str) -> Result<i64, ParseError> {
    let bad = || ParseError::InvalidValue(numeric.to_string());
    let (int_part, frac_part) = numeric.split_once('.').ok_or_else(bad)?;

    let frac_part = if frac_part.len() > 3 {
        log::warn!("precision loss truncating {numeric:?} to 3 fraction digits");
        &frac_part[..3]
    } else {
        frac_part
    };

    format!("{int_part}{frac_part:0<3}")
        .parse::<i64>()
        .map_err(|_| bad())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Same fields as the sample telegram in CLAUDE.md, minus the inline
    /// `# ...` comments (those are documentation annotations, not part of
    /// a real telegram) and with the CRLF line endings the meter actually
    /// sends. The CRC below was computed independently (CRC16/ARC over
    /// `/`..`!` inclusive) rather than copied from the doc, which uses an
    /// illustrative, not byte-exact, checksum.
    fn sample_telegram() -> String {
        let lines = [
            "/ISK5\\2M550T-1013",
            "",
            "1-3:0.2.8(50)",
            "0-0:1.0.0(260712163115S)",
            "1-0:1.8.1(005081.531*kWh)",
            "1-0:1.8.2(005930.099*kWh)",
            "1-0:2.8.1(003482.883*kWh)",
            "1-0:2.8.2(007913.281*kWh)",
            "1-0:1.7.0(00.000*kW)",
            "1-0:2.7.0(01.512*kW)",
            "1-0:21.7.0(00.083*kW)",
            "1-0:41.7.0(00.100*kW)",
            "1-0:61.7.0(00.000*kW)",
            "1-0:22.7.0(00.000*kW)",
            "1-0:42.7.0(00.000*kW)",
            "1-0:62.7.0(01.691*kW)",
            "0-1:24.2.1(260712163001S)(07731.454*m3)",
        ];
        format!("{}\r\n!C62E\r\n", lines.join("\r\n"))
    }

    // 2026-07-12 16:31:15 S (UTC+2) -> 2026-07-12 14:31:15 UTC
    const GLOBAL_TS: i64 = 1783866675;
    // 2026-07-12 16:30:01 S (UTC+2) -> 2026-07-12 14:30:01 UTC (gas meter's own stamp)
    const GAS_TS: i64 = 1783866601;

    #[test]
    fn parses_sample_telegram() {
        let telegram = parse_telegram(&sample_telegram()).expect("valid telegram should parse");
        assert_eq!(telegram.timestamp, GLOBAL_TS);
        let records = telegram.records;

        assert_eq!(
            records
                .iter()
                .find(|r| r.kind == "E-import-T1")
                .expect("E-import-T1 present"),
            &Record { kind: "E-import-T1", timestamp: GLOBAL_TS, value: 5_081_531 }
        );
        assert_eq!(
            records
                .iter()
                .find(|r| r.kind == "P-L1-import")
                .expect("P-L1-import present"),
            &Record { kind: "P-L1-import", timestamp: GLOBAL_TS, value: 83 }
        );
        // Gas record must carry its own embedded timestamp, not the telegram's.
        assert_eq!(
            records.iter().find(|r| r.kind == "G-import").expect("G-import present"),
            &Record { kind: "G-import", timestamp: GAS_TS, value: 7_731_454 }
        );

        // Version field (1-3:0.2.8) and the global-timestamp line itself
        // (0-0:1.0.0) are not stored as records.
        assert!(!records.iter().any(|r| r.timestamp == 0));
        assert_eq!(records.len(), 13);
    }

    #[test]
    fn rejects_bad_crc() {
        let telegram = sample_telegram().replace("C62E", "0000");
        assert!(matches!(
            parse_telegram(&telegram),
            Err(ParseError::CrcMismatch { .. })
        ));
    }

    #[test]
    fn rejects_missing_timestamp() {
        let telegram = sample_telegram().replace("0-0:1.0.0(260712163115S)\r\n", "");
        // CRC no longer matches once a line is removed, so patch it out too.
        let bang = telegram.find('!').unwrap();
        let body = &telegram[..bang + 1];
        let recomputed = crc16_arc(body.as_bytes());
        let telegram = format!("{body}{recomputed:04X}\r\n");

        assert_eq!(parse_telegram(&telegram), Err(ParseError::MissingTimestamp));
    }

    #[test]
    fn extract_groups_rejects_stray_characters_between_groups() {
        assert_eq!(extract_groups("(xx)(zz)"), Ok(vec!["xx", "zz"]));
        assert_eq!(
            extract_groups("(xx)yy(zz)"),
            Err(ParseError::MalformedLine("(xx)yy(zz)".to_string()))
        );
        assert_eq!(
            extract_groups("(xx"),
            Err(ParseError::MalformedLine("(xx".to_string()))
        );
    }

    #[test]
    fn rejects_line_with_stray_characters_between_groups() {
        let telegram = sample_telegram().replace(
            "0-1:24.2.1(260712163001S)(07731.454*m3)",
            "0-1:24.2.1(260712163001S)stray(07731.454*m3)",
        );
        let bang = telegram.find('!').unwrap();
        let body = &telegram[..bang + 1];
        let recomputed = crc16_arc(body.as_bytes());
        let telegram = format!("{body}{recomputed:04X}\r\n");

        assert!(matches!(
            parse_telegram(&telegram),
            Err(ParseError::MalformedLine(_))
        ));
    }

    #[test]
    fn scales_values_without_float_rounding() {
        assert_eq!(parse_scaled_int("005081.531").unwrap(), 5_081_531);
        assert_eq!(parse_scaled_int("00.083").unwrap(), 83);
        assert_eq!(parse_scaled_int("50.1").unwrap(), 50_100);
        assert_eq!(parse_scaled_int("50.1234").unwrap(), 50_123); // precision loss, chopped
    }
}
