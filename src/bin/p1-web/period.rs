use chrono::{Datelike, NaiveDate};

const SECONDS_PER_DAY: i64 = 86_400;

/// Real-world UTC offsets run from -12:00 to +14:00; anything outside that
/// is a malformed request, not a timezone.
const MAX_UTC_OFFSET_SECONDS: i64 = 14 * 3600;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Period {
    Day,
    Week,
    Month,
    Year,
    FiveYear,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Resolution {
    Minute,
    Day,
}

impl Period {
    pub fn parse(raw: &str) -> Option<Period> {
        match raw {
            "day" => Some(Period::Day),
            "week" => Some(Period::Week),
            "month" => Some(Period::Month),
            "year" => Some(Period::Year),
            "5year" => Some(Period::FiveYear),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Period::Day => "day",
            Period::Week => "week",
            Period::Month => "month",
            Period::Year => "year",
            Period::FiveYear => "5year",
        }
    }

    /// `day`/`week` need per-minute detail to be worth looking at; `month`
    /// and up would be thousands to tens of thousands of minute-points for
    /// a line chart, so they use the daily aggregate instead (see CLAUDE.md
    /// -- the day/month resolution choice this project already settled on).
    pub fn resolution(self) -> Resolution {
        match self {
            Period::Day | Period::Week => Resolution::Minute,
            Period::Month | Period::Year | Period::FiveYear => Resolution::Day,
        }
    }
}

fn midnight_utc(date: NaiveDate) -> i64 {
    date.and_hms_opt(0, 0, 0).expect("00:00:00 is always a valid time").and_utc().timestamp()
}

/// The calendar-aware `[start, end)` range of `period` containing `anchor`
/// (an epoch-second timestamp), in the caller's local calendar.
///
/// `utc_offset_seconds` is the client's local UTC offset (positive east of
/// UTC, e.g. `7200` for CEST) at the `anchor` instant, supplied by the
/// browser -- the server has no timezone database of its own, and a fixed
/// server-side zone would still need this same input to handle DST. All
/// calendar math below is done by shifting into "local-time-as-if-UTC" (add
/// the offset, do the day/week/month/year boundary math chrono already had,
/// then subtract the offset back off the result) rather than duplicating
/// that logic for a second calendar. A range that itself straddles a DST
/// transition (e.g. a week spanning the clock change) uses one offset for
/// the whole range -- a deliberate approximation, not worth a second offset
/// parameter for an hour of edge-case precision.
///
/// `None` if `anchor` isn't representable as a date at all, or the offset is
/// outside any real timezone (caller should treat that as a 400, not a panic
/// -- both come straight from an untrusted query string). Once both pass
/// those checks, `local_anchor` is bounded by chrono's date range and the
/// offset by a few hours, so none of the arithmetic below can overflow.
pub fn range(period: Period, anchor: i64, utc_offset_seconds: i64) -> Option<(i64, i64)> {
    if !(-MAX_UTC_OFFSET_SECONDS..=MAX_UTC_OFFSET_SECONDS).contains(&utc_offset_seconds) {
        return None;
    }
    let local_anchor = anchor.checked_add(utc_offset_seconds)?;
    let anchor_date = chrono::DateTime::from_timestamp(local_anchor, 0)?.date_naive();
    let to_utc = |local: i64| local - utc_offset_seconds;
    match period {
        Period::Day => {
            let local_start = local_anchor.div_euclid(SECONDS_PER_DAY) * SECONDS_PER_DAY;
            Some((to_utc(local_start), to_utc(local_start) + SECONDS_PER_DAY))
        }
        Period::Week => {
            let days_since_monday = i64::from(anchor_date.weekday().num_days_from_monday());
            let local_day_start = local_anchor.div_euclid(SECONDS_PER_DAY) * SECONDS_PER_DAY;
            let local_week_start = local_day_start - days_since_monday * SECONDS_PER_DAY;
            Some((to_utc(local_week_start), to_utc(local_week_start) + 7 * SECONDS_PER_DAY))
        }
        Period::Month => {
            let first_of_month = anchor_date.with_day(1)?;
            let next_month_first = if first_of_month.month() == 12 {
                NaiveDate::from_ymd_opt(first_of_month.year() + 1, 1, 1)?
            } else {
                NaiveDate::from_ymd_opt(first_of_month.year(), first_of_month.month() + 1, 1)?
            };
            Some((to_utc(midnight_utc(first_of_month)), to_utc(midnight_utc(next_month_first))))
        }
        Period::Year => {
            let year = anchor_date.year();
            let start = NaiveDate::from_ymd_opt(year, 1, 1)?;
            let end = NaiveDate::from_ymd_opt(year + 1, 1, 1)?;
            Some((to_utc(midnight_utc(start)), to_utc(midnight_utc(end))))
        }
        Period::FiveYear => {
            let year = anchor_date.year();
            let start = NaiveDate::from_ymd_opt(year - 4, 1, 1)?;
            let end = NaiveDate::from_ymd_opt(year + 1, 1, 1)?;
            Some((to_utc(midnight_utc(start)), to_utc(midnight_utc(end))))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // 2026-07-16 12:00:00 UTC, a Thursday. Reference epoch values below
    // independently verified via `date -u -j -f "%Y-%m-%d %H:%M:%S" ... +%s`,
    // not hand-computed, since hand arithmetic on epoch seconds is exactly
    // the kind of thing that's easy to get subtly wrong.
    const THURSDAY_NOON: i64 = 1_784_203_200;

    #[test]
    fn day_range_is_that_utc_calendar_day_at_zero_offset() {
        let (start, end) = range(Period::Day, THURSDAY_NOON, 0).unwrap();
        assert_eq!(start, 1_784_160_000); // 2026-07-16 00:00:00 UTC
        assert_eq!(end, start + SECONDS_PER_DAY);
    }

    #[test]
    fn day_range_shifts_to_the_local_calendar_day_with_a_nonzero_offset() {
        // THURSDAY_NOON is 2026-07-16 12:00:00 UTC == 14:00 CEST, still
        // comfortably inside the same local day. Local midnight (00:00
        // CEST) on the 16th is 2026-07-15 22:00:00 UTC -- i.e. the UTC
        // boundary shifts *earlier* by the offset, since UTC = local - offset.
        let cest_offset = 2 * 3600;
        let (start, end) = range(Period::Day, THURSDAY_NOON, cest_offset).unwrap();
        assert_eq!(start, 1_784_160_000 - cest_offset); // 2026-07-15 22:00:00 UTC
        assert_eq!(end, start + SECONDS_PER_DAY);
    }

    #[test]
    fn day_range_with_offset_can_pick_the_previous_local_day() {
        // 2026-07-16 00:30:00 UTC == 2026-07-15 20:30 local (-4h offset):
        // in UTC this instant is already "the 16th", but locally it's still
        // "the 15th" -- the whole reason `range` needs the offset at all.
        // Local midnight (00:00) on the 15th is 2026-07-15 04:00:00 UTC
        // (UTC = local + 4h at this offset).
        let just_after_utc_midnight = 1_784_160_000 + 1800;
        let negative_offset = -4 * 3600;
        let (start, end) = range(Period::Day, just_after_utc_midnight, negative_offset).unwrap();
        assert_eq!(start, 1_784_160_000 - SECONDS_PER_DAY - negative_offset); // 2026-07-15 04:00:00 UTC
        assert_eq!(end, start + SECONDS_PER_DAY);
    }

    #[test]
    fn week_range_starts_on_monday() {
        let (start, end) = range(Period::Week, THURSDAY_NOON, 0).unwrap();
        assert_eq!(start, 1_783_900_800); // 2026-07-13 00:00:00 UTC, the Monday
        assert_eq!(end, start + 7 * SECONDS_PER_DAY);
    }

    #[test]
    fn month_range_handles_variable_month_length() {
        let (start, end) = range(Period::Month, THURSDAY_NOON, 0).unwrap();
        assert_eq!(start, 1_782_864_000); // 2026-07-01 00:00:00 UTC
        assert_eq!(end, 1_785_542_400); // 2026-08-01 00:00:00 UTC (31-day July)
    }

    #[test]
    fn year_range_is_a_full_calendar_year() {
        let (start, end) = range(Period::Year, THURSDAY_NOON, 0).unwrap();
        assert_eq!(start, 1_767_225_600); // 2026-01-01 00:00:00 UTC
        assert_eq!(end, 1_798_761_600); // 2027-01-01 00:00:00 UTC
    }

    #[test]
    fn five_year_range_ends_in_anchors_year() {
        let (start, end) = range(Period::FiveYear, THURSDAY_NOON, 0).unwrap();
        assert_eq!(start, 1_640_995_200); // 2022-01-01 00:00:00 UTC
        assert_eq!(end, 1_798_761_600); // 2027-01-01 00:00:00 UTC
    }

    #[test]
    fn resolution_matches_the_established_day_month_split() {
        assert!(Period::Day.resolution() == Resolution::Minute);
        assert!(Period::Week.resolution() == Resolution::Minute);
        assert!(Period::Month.resolution() == Resolution::Day);
        assert!(Period::Year.resolution() == Resolution::Day);
        assert!(Period::FiveYear.resolution() == Resolution::Day);
    }

    #[test]
    fn out_of_range_input_is_rejected_instead_of_overflowing() {
        assert!(range(Period::Day, i64::MAX, 1).is_none());
        assert!(range(Period::Day, i64::MIN, -1).is_none());
        assert!(range(Period::Day, THURSDAY_NOON, i64::MIN).is_none());
        assert!(range(Period::Day, THURSDAY_NOON, MAX_UTC_OFFSET_SECONDS + 1).is_none());
    }

    #[test]
    fn parse_rejects_unknown_period() {
        assert!(Period::parse("fortnight").is_none());
    }
}
