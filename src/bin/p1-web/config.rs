use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use std::path::Path;

/// p1-web's own settings, from the JSON file passed via `--config`. Read once
/// at startup: a broken file is a misconfiguration, so it stops p1-web from
/// starting rather than surfacing later as a half-working dashboard.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    /// Absent: no prices configured, so the dashboard shows no euro amounts.
    pub tariffs: Option<Vec<TariffPeriod>>,
}

/// One contract period's prices. Together the periods cover the whole
/// timeline without gaps or overlaps (see `validate`), so every hour of data
/// has exactly one applicable price. Days are the viewer's local calendar
/// days -- the browser, which knows the local timezone, does the matching.
#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TariffPeriod {
    /// First day this period applies, inclusive. Only the first period has
    /// none: it covers everything before its `until`.
    pub from: Option<NaiveDate>,
    /// Last day this period applies, inclusive. Only the last period has
    /// none: it covers everything from its `from` on.
    pub until: Option<NaiveDate>,
    pub electricity: ElectricityPrices,
    /// € per m³.
    pub gas: f64,
}

/// € per kWh. Not sign-checked: a negative price is a real contract term
/// (e.g. paying to deliver back), not a typo.
#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ElectricityPrices {
    pub import_t1: f64,
    pub import_t2: f64,
    pub export_t1: f64,
    pub export_t2: f64,
}

pub fn load(path: &Path) -> Result<Config, String> {
    let text = std::fs::read_to_string(path).map_err(|err| format!("can't read {}: {err}", path.display()))?;
    let config: Config = serde_json::from_str(&text).map_err(|err| format!("{}: {err}", path.display()))?;
    if let Some(tariffs) = &config.tariffs {
        validate(tariffs).map_err(|err| format!("{}: {err}", path.display()))?;
    }
    Ok(config)
}

/// Periods are numbered from 1 in messages, as they'd be counted reading the
/// file top to bottom.
fn validate(tariffs: &[TariffPeriod]) -> Result<(), String> {
    let (Some(first), Some(last)) = (tariffs.first(), tariffs.last()) else {
        return Err("\"tariffs\" is empty -- leave it out entirely to show no prices".to_string());
    };
    if let Some(from) = first.from {
        return Err(format!("tariff period 1 must not have a \"from\" (it covers everything before its \"until\"), got {from}"));
    }
    if let Some(until) = last.until {
        return Err(format!(
            "tariff period {} must not have an \"until\" (it covers everything from its \"from\" on), got {until}",
            tariffs.len()
        ));
    }
    for (i, period) in tariffs.iter().enumerate() {
        if let (Some(from), Some(until)) = (period.from, period.until)
            && from > until
        {
            return Err(format!("tariff period {}: \"from\" {from} is after \"until\" {until}", i + 1));
        }
    }
    for (i, pair) in tariffs.windows(2).enumerate() {
        let (Some(until), Some(next_from)) = (pair[0].until, pair[1].from) else {
            return Err(format!(
                "tariff periods {} and {} must be joined by an \"until\" and a \"from\"",
                i + 1,
                i + 2
            ));
        };
        if until.succ_opt() != Some(next_from) {
            return Err(format!(
                "tariff period {} must start the day after period {} ends ({until}), got {next_from}",
                i + 2,
                i + 1
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(json: &str) -> Result<Config, String> {
        let config: Config = serde_json::from_str(json).map_err(|err| err.to_string())?;
        if let Some(tariffs) = &config.tariffs {
            validate(tariffs)?;
        }
        Ok(config)
    }

    fn period(from: Option<&str>, until: Option<&str>) -> String {
        let date = |d: Option<&str>| d.map_or("null".to_string(), |d| format!("\"{d}\""));
        format!(
            r#"{{"from": {}, "until": {}, "electricity": {{"import_t1": 0.21, "import_t2": 0.24, "export_t1": 0.2, "export_t2": 0.2}}, "gas": 1.25}}"#,
            date(from),
            date(until)
        )
    }

    fn tariffs(periods: &[String]) -> String {
        format!(r#"{{"tariffs": [{}]}}"#, periods.join(","))
    }

    #[test]
    fn no_tariffs_key_means_no_prices() {
        assert!(parse("{}").unwrap().tariffs.is_none());
    }

    #[test]
    fn a_single_open_ended_period_is_valid() {
        assert_eq!(parse(&tariffs(&[period(None, None)])).unwrap().tariffs.unwrap().len(), 1);
    }

    #[test]
    fn contiguous_periods_are_valid() {
        let json = tariffs(&[
            period(None, Some("2025-12-31")),
            period(Some("2026-01-01"), Some("2026-06-30")),
            period(Some("2026-07-01"), None),
        ]);
        assert!(parse(&json).is_ok());
    }

    #[test]
    fn missing_from_or_until_keys_count_as_absent() {
        let json = r#"{"tariffs": [{"electricity": {"import_t1": 0.21, "import_t2": 0.24, "export_t1": 0.2, "export_t2": 0.2}, "gas": 1.25}]}"#;
        assert!(parse(json).is_ok());
    }

    #[test]
    fn rejects_an_empty_tariffs_array() {
        assert!(parse(r#"{"tariffs": []}"#).unwrap_err().contains("empty"));
    }

    #[test]
    fn rejects_a_from_on_the_first_period() {
        let err = parse(&tariffs(&[period(Some("2026-01-01"), None)])).unwrap_err();
        assert!(err.contains("period 1 must not have a \"from\""), "{err}");
    }

    #[test]
    fn rejects_an_until_on_the_last_period() {
        let err = parse(&tariffs(&[period(None, Some("2026-12-31"))])).unwrap_err();
        assert!(err.contains("must not have an \"until\""), "{err}");
    }

    #[test]
    fn rejects_a_gap_between_periods() {
        let json = tariffs(&[period(None, Some("2025-12-31")), period(Some("2026-01-02"), None)]);
        assert!(parse(&json).unwrap_err().contains("must start the day after"));
    }

    #[test]
    fn rejects_an_overlap_between_periods() {
        let json = tariffs(&[period(None, Some("2025-12-31")), period(Some("2025-12-31"), None)]);
        assert!(parse(&json).unwrap_err().contains("must start the day after"));
    }

    #[test]
    fn rejects_an_inner_period_missing_its_bounds() {
        let json = tariffs(&[period(None, Some("2025-12-31")), period(Some("2026-01-01"), None), period(None, None)]);
        assert!(parse(&json).unwrap_err().contains("must be joined by"));
    }

    #[test]
    fn rejects_an_inner_period_ending_before_it_starts() {
        let json = tariffs(&[
            period(None, Some("2025-12-31")),
            period(Some("2026-01-01"), Some("2025-06-30")),
            period(Some("2025-07-01"), None),
        ]);
        assert!(parse(&json).unwrap_err().contains("is after"));
    }

    #[test]
    fn rejects_unknown_fields_and_invalid_dates() {
        assert!(parse(r#"{"tarifs": []}"#).is_err(), "a typo'd key must not silently mean \"no prices\"");
        assert!(parse(&tariffs(&[period(None, Some("2026-02-30")), period(Some("2026-03-01"), None)])).is_err());
    }
}
