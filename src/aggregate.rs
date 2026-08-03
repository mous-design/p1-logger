use rusqlite::Connection;
use std::collections::HashMap;
use std::path::Path;

const SECONDS_PER_MINUTE: i64 = 60;
const SECONDS_PER_HOUR: i64 = 3_600;
const SECONDS_PER_DAY: i64 = 86_400;

pub struct PowerBucket {
    pub bucket_start: i64,
    pub min_w: i64,
    pub avg_w: i64,
    pub max_w: i64,
}

pub struct ElectricityMeterBucket {
    pub bucket_start: i64,
    pub net_t1_wh: i64,
    pub net_t2_wh: i64,
}

pub struct GasMeterBucket {
    pub bucket_start: i64,
    pub import_dm3: i64,
}

/// Everything computed for one calendar day, ready to write in one transaction.
pub struct DayAggregate {
    pub power_1min: Vec<PowerBucket>,
    pub power_1day: Vec<PowerBucket>,
    pub meter_electricity: Vec<ElectricityMeterBucket>,
    pub meter_gas: Vec<GasMeterBucket>,
}

/// Reads every (timestamp, value) row for one OBIS-derived type, in
/// timestamp order.
fn read_series_sorted(conn: &Connection, kind: &str) -> rusqlite::Result<Vec<(i64, i64)>> {
    let mut stmt = conn.prepare("SELECT timestamp, value FROM readings WHERE type = ?1 ORDER BY timestamp")?;
    stmt.query_map([kind], |row| Ok((row.get(0)?, row.get(1)?)))?.collect()
}

/// Collapses consecutive rows sharing the same timestamp down to the last
/// one seen for it. `series` must already be sorted by timestamp (as
/// `read_series_sorted` guarantees), so duplicates are always adjacent.
///
/// Real cause, confirmed by inspecting the raw data behind a batch of
/// "no matching export" warnings: telegram timing jitter occasionally lands
/// two transmissions on the same labelled second, while the neighbouring
/// second goes empty -- the same underlying jitter as the long-known
/// "missing seconds" gaps, just showing up as a double instead of a gap
/// this time. Without this, a duplicate silently desyncs `net_series_sorted`
/// even though the intent (keep the freshest reading) is unambiguous.
fn dedup_last_per_timestamp(series: Vec<(i64, i64)>) -> Vec<(i64, i64)> {
    let mut result: Vec<(i64, i64)> = Vec::with_capacity(series.len());
    for (timestamp, value) in series {
        match result.last_mut() {
            Some((last_timestamp, last_value)) if *last_timestamp == timestamp => *last_value = value,
            _ => result.push((timestamp, value)),
        }
    }
    result
}

/// Pairs up `import_kind`/`export_kind` rows sharing the same timestamp into
/// a signed, timestamp-ordered net series (positive = import, negative =
/// export). Used for net power (P-import/P-export) and, identically, for
/// net electricity-meter consumption per tariff (E-import-T1/E-export-T1,
/// and T2) -- import and export are mutually exclusive at any instant, so
/// netting them the same way makes sense regardless of which pair it is.
/// Every valid telegram carries both fields of a pair with the same
/// timestamp, so a value on one side with nothing on the other is
/// unexpected.
///
/// `quiet` suppresses the anomaly warnings below without changing what gets
/// computed -- see `write_day`'s doc comment for why: today's still-growing
/// file gets recomputed hourly, so every anomaly in it (real or just a
/// symptom of the day being incomplete) would otherwise get logged again
/// and again, all day, for the exact same underlying cause.
fn net_series_sorted(
    conn: &Connection,
    import_kind: &str,
    export_kind: &str,
    quiet: bool,
) -> rusqlite::Result<Vec<(i64, i64)>> {
    let imports = dedup_last_per_timestamp(read_series_sorted(conn, import_kind)?);
    // For exports: convert vec to hashmap for O(1) lookup. Explicit dedup
    // here too even though collect() into a HashMap already keeps only the
    // last value per key by itself -- doing it via the same named function
    // as imports makes that "last wins" choice visible, not an incidental
    // side effect of picking a HashMap.
    let mut exports: HashMap<i64, i64> =
        dedup_last_per_timestamp(read_series_sorted(conn, export_kind)?).into_iter().collect();

    let mut net = Vec::with_capacity(imports.len());
    for (timestamp, import) in imports {
        match exports.remove(&timestamp) {
            Some(export) => net.push((timestamp, import - export)),
            None if quiet => {}
            None => log::warn!("{import_kind} at {timestamp} has no matching {export_kind}; skipped from net aggregates"),
        }
    }
    if !quiet {
        for orphan_timestamp in exports.into_keys() {
            log::warn!("{export_kind} at {orphan_timestamp} has no matching {import_kind}; skipped from net aggregates");
        }
    }
    if !net.is_sorted_by_key(|(timestamp, _)| *timestamp) {
        if !quiet {
            log::warn!(
                "net {import_kind}/{export_kind} series wasn't sorted by timestamp as expected; \
                 downstream bucketing assumes ascending order, so sorting now to avoid silently wrong buckets"
            );
        }
        net.sort_unstable_by_key(|(timestamp, _)| *timestamp);
    }
    Ok(net)
}

/// Buckets an ordered (timestamp, value) series into fixed-size,
/// bucket-aligned windows with min/avg/max. Used for both `power_1min`
/// (bucket_size = 60) and `power_1day` (bucket_size = 86_400) -- same
/// computation, just a different window.
pub fn bucket_power(series: &[(i64, i64)], bucket_size: i64) -> Vec<PowerBucket> {
    let mut buckets: Vec<PowerBucket> = Vec::new();
    let mut current_start: i64 = i64::MIN; // i64::MIN as "no bucket started yet" sentinel
    let mut min_w = i64::MAX;
    let mut max_w = i64::MIN;
    let mut sum: i64 = 0;
    let mut count: i64 = 0;

    for &(timestamp, value) in series {
        let bucket_start = timestamp.div_euclid(bucket_size) * bucket_size;
        if bucket_start != current_start {
            if count > 0 {
                buckets.push(PowerBucket { bucket_start: current_start, min_w, avg_w: sum.div_euclid(count), max_w });
            }
            current_start = bucket_start;
            min_w = value;
            max_w = value;
            sum = value;
            count = 1;
        } else {
            min_w = min_w.min(value);
            max_w = max_w.max(value);
            sum += value;
            count += 1;
        }
    }
    if count > 0 {
        buckets.push(PowerBucket { bucket_start: current_start, min_w, avg_w: sum.div_euclid(count), max_w });
    }
    buckets
}

/// The last (highest-timestamp) value per hour bucket -- a snapshot, not an
/// aggregate: a cumulative meter reading only ever needs "what did it read
/// at the end of this hour", not min/avg/max. Same track-current-then-
/// commit-on-transition shape as `bucket_power`: `series` is sorted, so only
/// the value right before an hour changes ever needs to be committed, not
/// every reading along the way.
fn last_value_per_hour(series: Vec<(i64, i64)>) -> HashMap<i64, i64> {
    let mut last: HashMap<i64, i64> = HashMap::new();
    let mut current_start: i64 = i64::MIN; // sentinel: no hour started yet
    let mut current_value: i64 = 0;

    for (timestamp, value) in series {
        let bucket_start = timestamp.div_euclid(SECONDS_PER_HOUR) * SECONDS_PER_HOUR;
        if bucket_start != current_start {
            if current_start != i64::MIN {
                last.insert(current_start, current_value);
            }
            current_start = bucket_start;
        }
        current_value = value;
    }
    if current_start != i64::MIN {
        last.insert(current_start, current_value);
    }
    last
}

fn electricity_meter_buckets(conn: &Connection, quiet: bool) -> rusqlite::Result<Vec<ElectricityMeterBucket>> {
    let net_t1 = last_value_per_hour(net_series_sorted(conn, "E-import-T1", "E-export-T1", quiet)?);
    let net_t2 = last_value_per_hour(net_series_sorted(conn, "E-import-T2", "E-export-T2", quiet)?);

    // Union of both hour-sets: T1 and T2 normally cover the same hours (one
    // telegram contributes to both every time), but an hour where every
    // reading on one side failed to pair (see `net_series_sorted`) would
    // leave that hour missing from just one of the two maps -- so every hour
    // that appears in *either* needs to be considered, not just one's own.
    let mut hours: Vec<i64> = net_t1.keys().chain(net_t2.keys()).copied().collect();
    hours.sort_unstable();
    hours.dedup();

    Ok(hours
        .into_iter()
        .filter_map(|bucket_start| match (net_t1.get(&bucket_start), net_t2.get(&bucket_start)) {
            (Some(&net_t1_wh), Some(&net_t2_wh)) => Some(ElectricityMeterBucket { bucket_start, net_t1_wh, net_t2_wh }),
            _ => {
                if !quiet {
                    log::warn!(
                        "hour {bucket_start} is missing net T1 or net T2 electricity consumption; \
                         skipped from meter_electricity aggregates"
                    );
                }
                None
            }
        })
        .collect())
}

fn gas_meter_buckets(conn: &Connection) -> rusqlite::Result<Vec<GasMeterBucket>> {
    let mut hours: Vec<(i64, i64)> = last_value_per_hour(read_series_sorted(conn, "G-import")?).into_iter().collect();
    hours.sort_unstable_by_key(|(bucket_start, _)| *bucket_start);
    Ok(hours
        .into_iter()
        .map(|(bucket_start, import_dm3)| GasMeterBucket { bucket_start, import_dm3 })
        .collect())
}

/// Computes every aggregate for one raw day-file's connection. `quiet`: see
/// `write_day`'s doc comment -- pass `true` while a day is still "today"
/// (recomputed hourly, so its own anomalies would otherwise repeat all day),
/// `false` for its one-time, final computation right before archiving.
pub fn compute_day(conn: &Connection, quiet: bool) -> rusqlite::Result<DayAggregate> {
    let net = net_series_sorted(conn, "P-import", "P-export", quiet)?;
    Ok(DayAggregate {
        power_1min: bucket_power(&net, SECONDS_PER_MINUTE),
        power_1day: bucket_power(&net, SECONDS_PER_DAY),
        meter_electricity: electricity_meter_buckets(conn, quiet)?,
        meter_gas: gas_meter_buckets(conn)?,
    })
}

pub fn open_aggregates_db(path: &Path) -> rusqlite::Result<Connection> {
    let conn = Connection::open(path)?;
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.pragma_update(None, "synchronous", "NORMAL")?;
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS power_1min (
            bucket_start INTEGER NOT NULL PRIMARY KEY,
            min_w INTEGER NOT NULL,
            avg_w INTEGER NOT NULL,
            max_w INTEGER NOT NULL
        );
        CREATE TABLE IF NOT EXISTS power_1day (
            bucket_start INTEGER NOT NULL PRIMARY KEY,
            min_w INTEGER NOT NULL,
            avg_w INTEGER NOT NULL,
            max_w INTEGER NOT NULL
        );
        CREATE TABLE IF NOT EXISTS meter_electricity (
            bucket_start INTEGER NOT NULL PRIMARY KEY,
            net_t1_wh INTEGER NOT NULL,
            net_t2_wh INTEGER NOT NULL
        );
        CREATE TABLE IF NOT EXISTS meter_gas (
            bucket_start INTEGER NOT NULL PRIMARY KEY,
            import_dm3 INTEGER NOT NULL
        );",
    )?;
    Ok(conn)
}

fn in_day_range(bucket_start: i64, day_start: i64, day_end: i64) -> bool {
    bucket_start >= day_start && bucket_start < day_end
}

/// How far outside `[day_start, day_end)` a bucket_start sits, in seconds.
/// Only meaningful (and only ever called) when `bucket_start` is already
/// known to be outside that range.
fn distance_outside_range(bucket_start: i64, day_start: i64, day_end: i64) -> i64 {
    if bucket_start < day_start { day_start - bucket_start } else { bucket_start - day_end }
}

/// Gas readings carry their own embedded timestamp (see `db.rs`), which can
/// lag behind the telegram timestamp that decided which day-file they were
/// written to -- the meter's M-Bus push only updates the gas value/timestamp
/// occasionally, so every telegram in between keeps repeating the same
/// stale one. A single hand-picked example first suggested ~5 minutes of
/// lag, but re-running against 19 real days showed the actual pattern is
/// much more mechanical: the meter updates gas on a fixed hourly schedule,
/// so right after midnight the last-known reading is consistently exactly
/// ~1 hour stale (the previous day's 23:00 update), every single day, not
/// just occasionally. 2 hours of tolerance covers that with real room to
/// spare, while still flagging anything further off (which would point at
/// a real bug, not this expected lag) as an actual anomaly.
const GAS_BOUNDARY_TOLERANCE_SECONDS: i64 = 2 * SECONDS_PER_HOUR;

/// Replaces one day's worth of aggregate rows in a single transaction:
/// idempotent by construction (delete then insert, not "insert if
/// missing"), so re-running after a crash -- or just re-running at all --
/// always converges to the same correct state instead of needing to reason
/// about partial completion.
///
/// Buckets outside `[day_start, day_end)` are dropped, not inserted: a
/// record can land in the "wrong" day-file by a few seconds when its own
/// timestamp (e.g. the gas reading) disagrees with the telegram timestamp
/// that decided which file it was written to (see `db.rs`). Such a stray
/// record only ever covers a sliver of its real bucket -- the neighbouring
/// day-file holds the rest of that hour/minute and produces the real,
/// complete bucket for it. Inserting the stray one anyway would either
/// collide with that real row (UNIQUE constraint failure) or silently
/// overwrite it with a far-less-complete value.
///
/// `power_1min`/`power_1day`/`meter_electricity` always warn when this
/// happens -- there's no legitimate reason for those to ever land near a
/// boundary, since they're all bucketed by the telegram's own timestamp.
/// `meter_gas` is the one exception: its own timestamp is expected to lag
/// the telegram's by up to `GAS_BOUNDARY_TOLERANCE_SECONDS`, so that much
/// drift is dropped silently, and only anything further out (a real
/// anomaly) gets logged.
///
/// `quiet` suppresses all of the above warnings (not the dropping itself,
/// which always happens regardless): a day that's still "today" gets
/// recomputed and rewritten every hour by the cron job, so anything
/// anomalous in it -- real or just a symptom of the day being incomplete --
/// would otherwise get logged again on every single one of those hourly
/// runs, all pointing at the exact same underlying cause. Pass `true` while
/// `day_start >= today_start`, `false` for a day's one-time, final write
/// right before it gets archived -- that way every real anomaly still
/// surfaces exactly once, on the run where it actually matters, instead of
/// up to ~24 times or not at all.
pub fn write_day(
    aggregates_conn: &mut Connection,
    day_start: i64,
    day_end: i64,
    data: &DayAggregate,
    quiet: bool,
) -> rusqlite::Result<()> {
    let tx = aggregates_conn.transaction()?;
    {
        tx.execute("DELETE FROM power_1min WHERE bucket_start >= ?1 AND bucket_start < ?2", (day_start, day_end))?;
        tx.execute("DELETE FROM power_1day WHERE bucket_start >= ?1 AND bucket_start < ?2", (day_start, day_end))?;
        tx.execute("DELETE FROM meter_electricity WHERE bucket_start >= ?1 AND bucket_start < ?2", (day_start, day_end))?;
        tx.execute("DELETE FROM meter_gas WHERE bucket_start >= ?1 AND bucket_start < ?2", (day_start, day_end))?;

        let mut power_1min_stmt =
            tx.prepare_cached("INSERT INTO power_1min (bucket_start, min_w, avg_w, max_w) VALUES (?1, ?2, ?3, ?4)")?;
        for bucket in &data.power_1min {
            if !in_day_range(bucket.bucket_start, day_start, day_end) {
                if !quiet {
                    log::warn!("power_1min bucket {} outside [{day_start}, {day_end}); dropped", bucket.bucket_start);
                }
                continue;
            }
            power_1min_stmt.execute((bucket.bucket_start, bucket.min_w, bucket.avg_w, bucket.max_w))?;
        }
        drop(power_1min_stmt);

        let mut power_1day_stmt =
            tx.prepare_cached("INSERT INTO power_1day (bucket_start, min_w, avg_w, max_w) VALUES (?1, ?2, ?3, ?4)")?;
        for bucket in &data.power_1day {
            if !in_day_range(bucket.bucket_start, day_start, day_end) {
                if !quiet {
                    log::warn!("power_1day bucket {} outside [{day_start}, {day_end}); dropped", bucket.bucket_start);
                }
                continue;
            }
            power_1day_stmt.execute((bucket.bucket_start, bucket.min_w, bucket.avg_w, bucket.max_w))?;
        }
        drop(power_1day_stmt);

        let mut electricity_stmt = tx.prepare_cached(
            "INSERT INTO meter_electricity (bucket_start, net_t1_wh, net_t2_wh) VALUES (?1, ?2, ?3)",
        )?;
        for bucket in &data.meter_electricity {
            if !in_day_range(bucket.bucket_start, day_start, day_end) {
                if !quiet {
                    log::warn!("meter_electricity bucket {} outside [{day_start}, {day_end}); dropped", bucket.bucket_start);
                }
                continue;
            }
            electricity_stmt.execute((bucket.bucket_start, bucket.net_t1_wh, bucket.net_t2_wh))?;
        }
        drop(electricity_stmt);

        let mut gas_stmt = tx.prepare_cached("INSERT INTO meter_gas (bucket_start, import_dm3) VALUES (?1, ?2)")?;
        for bucket in &data.meter_gas {
            if !in_day_range(bucket.bucket_start, day_start, day_end) {
                let distance = distance_outside_range(bucket.bucket_start, day_start, day_end);
                if distance > GAS_BOUNDARY_TOLERANCE_SECONDS && !quiet {
                    log::warn!(
                        "meter_gas bucket {} outside [{day_start}, {day_end}) by {distance}s -- \
                         further than the expected M-Bus lag near a day boundary; dropped",
                        bucket.bucket_start
                    );
                }
                continue;
            }
            gas_stmt.execute((bucket.bucket_start, bucket.import_dm3))?;
        }
    }
    tx.commit()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn buckets_min_avg_max_per_window() {
        let series = vec![(0, 10), (10, 20), (30, 30), (60, 100), (90, 200)];
        let buckets = bucket_power(&series, 60);

        assert_eq!(buckets.len(), 2);
        assert_eq!(buckets[0].bucket_start, 0);
        assert_eq!(buckets[0].min_w, 10);
        assert_eq!(buckets[0].max_w, 30);
        assert_eq!(buckets[0].avg_w, (10 + 20 + 30) / 3);
        assert_eq!(buckets[1].bucket_start, 60);
        assert_eq!(buckets[1].min_w, 100);
        assert_eq!(buckets[1].max_w, 200);
        assert_eq!(buckets[1].avg_w, (100 + 200) / 2);
    }

    #[test]
    fn negative_timestamps_bucket_correctly_via_div_euclid() {
        // -1 belongs to the bucket starting at -60, not 0: div_euclid
        // floors toward negative infinity, matching how day-bucketing
        // already works elsewhere in this project.
        let series = vec![(-1, 5)];
        let buckets = bucket_power(&series, 60);
        assert_eq!(buckets[0].bucket_start, -60);
    }

    #[test]
    fn empty_series_produces_no_buckets() {
        assert!(bucket_power(&[], 60).is_empty());
    }

    #[test]
    fn day_bucket_size_gives_one_bucket_for_a_whole_day() {
        let series = vec![(0, 100), (43_200, 300), (86_399, 50)];
        let buckets = bucket_power(&series, SECONDS_PER_DAY);
        assert_eq!(buckets.len(), 1);
        assert_eq!(buckets[0].min_w, 50);
        assert_eq!(buckets[0].max_w, 300);
    }

    #[test]
    fn schema_creates_all_four_tables() {
        let path = std::env::temp_dir().join(format!("p1-aggregate-schema-test-{}.sqlite3", std::process::id()));
        let _ = std::fs::remove_file(&path);

        let conn = open_aggregates_db(&path).unwrap();
        for table in ["power_1min", "power_1day", "meter_electricity", "meter_gas"] {
            let count: i64 = conn
                .query_row("SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name = ?1", [table], |row| {
                    row.get(0)
                })
                .unwrap();
            assert_eq!(count, 1, "expected table {table} to exist");
        }
        drop(conn);
        std::fs::remove_file(&path).unwrap();
    }

    #[test]
    fn write_day_is_idempotent_on_rerun() {
        let path = std::env::temp_dir().join(format!("p1-aggregate-idempotent-test-{}.sqlite3", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let mut conn = open_aggregates_db(&path).unwrap();

        let data = DayAggregate {
            power_1min: vec![PowerBucket { bucket_start: 0, min_w: 1, avg_w: 2, max_w: 3 }],
            power_1day: vec![PowerBucket { bucket_start: 0, min_w: 1, avg_w: 2, max_w: 3 }],
            meter_electricity: vec![],
            meter_gas: vec![],
        };

        write_day(&mut conn, 0, SECONDS_PER_DAY, &data, false).unwrap();
        write_day(&mut conn, 0, SECONDS_PER_DAY, &data, false).unwrap();

        let count: i64 = conn.query_row("SELECT COUNT(*) FROM power_1min", [], |row| row.get(0)).unwrap();
        assert_eq!(count, 1, "rerunning must not duplicate rows");

        drop(conn);
        std::fs::remove_file(&path).unwrap();
    }

    #[test]
    fn write_day_drops_buckets_outside_its_own_range_instead_of_colliding() {
        // A stray boundary record (e.g. a gas reading whose own timestamp
        // disagrees with the telegram timestamp that filed it) can produce a
        // computed bucket for the *next* day. Simulates that: day 0 computes
        // one in-range bucket plus one stray bucket landing exactly at the
        // next day's bucket_start.
        let path = std::env::temp_dir()
            .join(format!("p1-aggregate-boundary-test-{}.sqlite3", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let mut conn = open_aggregates_db(&path).unwrap();

        let day0 = DayAggregate {
            power_1min: vec![],
            power_1day: vec![],
            meter_electricity: vec![],
            meter_gas: vec![
                GasMeterBucket { bucket_start: 0, import_dm3: 100 },
                GasMeterBucket { bucket_start: SECONDS_PER_DAY, import_dm3: 999 }, // stray, belongs to day 1
            ],
        };
        write_day(&mut conn, 0, SECONDS_PER_DAY, &day0, false).unwrap();

        // Day 1's own, correct, in-range value for that same bucket_start.
        let day1 = DayAggregate {
            power_1min: vec![],
            power_1day: vec![],
            meter_electricity: vec![],
            meter_gas: vec![GasMeterBucket { bucket_start: SECONDS_PER_DAY, import_dm3: 200 }],
        };
        write_day(&mut conn, SECONDS_PER_DAY, 2 * SECONDS_PER_DAY, &day1, false).unwrap();

        let count: i64 = conn.query_row("SELECT COUNT(*) FROM meter_gas", [], |row| row.get(0)).unwrap();
        assert_eq!(count, 2, "one row per real day, stray boundary bucket must not add a third");

        let value: i64 = conn
            .query_row("SELECT import_dm3 FROM meter_gas WHERE bucket_start = ?1", [SECONDS_PER_DAY], |row| row.get(0))
            .unwrap();
        assert_eq!(value, 200, "day 1's real value must win, not day 0's stray one");

        drop(conn);
        std::fs::remove_file(&path).unwrap();
    }

    #[test]
    fn distance_outside_range_measures_from_the_nearest_boundary() {
        let day_start = 0;
        let day_end = SECONDS_PER_DAY;

        assert_eq!(distance_outside_range(-1, day_start, day_end), 1, "1s before day_start");
        assert_eq!(distance_outside_range(-3600, day_start, day_end), 3600, "1h before day_start");
        assert_eq!(distance_outside_range(day_end, day_start, day_end), 0, "exactly at day_end");
        assert_eq!(distance_outside_range(day_end + 3600, day_start, day_end), 3600, "1h past day_end");
    }

    #[test]
    fn write_day_drops_far_out_of_range_gas_bucket_and_would_warn_unlike_the_expected_boundary_lag() {
        // Not directly assertable (no test logger installed), but this
        // locks in the *data*-level behaviour that must hold regardless of
        // whether the "beyond tolerance" branch logs: a gas bucket far
        // outside the day's range is dropped, exactly like an in-tolerance
        // one -- the tolerance only controls whether it's also logged.
        let path = std::env::temp_dir()
            .join(format!("p1-aggregate-gas-tolerance-test-{}.sqlite3", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let mut conn = open_aggregates_db(&path).unwrap();

        let data = DayAggregate {
            power_1min: vec![],
            power_1day: vec![],
            meter_electricity: vec![],
            meter_gas: vec![
                GasMeterBucket { bucket_start: 0, import_dm3: 100 },
                // Days off, not a boundary-lag case -- must still be dropped.
                GasMeterBucket { bucket_start: 10 * SECONDS_PER_DAY, import_dm3: 999 },
            ],
        };
        write_day(&mut conn, 0, SECONDS_PER_DAY, &data, false).unwrap();

        let count: i64 = conn.query_row("SELECT COUNT(*) FROM meter_gas", [], |row| row.get(0)).unwrap();
        assert_eq!(count, 1, "the far-out-of-range bucket must still be dropped, not inserted");

        drop(conn);
        std::fs::remove_file(&path).unwrap();
    }

    #[test]
    fn electricity_meter_buckets_nets_import_minus_export_per_tariff() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("CREATE TABLE readings (type TEXT NOT NULL, timestamp INTEGER NOT NULL, value INTEGER NOT NULL);")
            .unwrap();
        let insert = |kind: &str, timestamp: i64, value: i64| {
            conn.execute("INSERT INTO readings (type, timestamp, value) VALUES (?1, ?2, ?3)", (kind, timestamp, value))
                .unwrap();
        };

        insert("E-import-T1", 0, 100);
        insert("E-export-T1", 0, 10);
        insert("E-import-T2", 0, 200);
        insert("E-export-T2", 0, 20);

        let buckets = electricity_meter_buckets(&conn, false).unwrap();

        assert_eq!(buckets.len(), 1);
        assert_eq!(buckets[0].bucket_start, 0);
        assert_eq!(buckets[0].net_t1_wh, 90, "100 import - 10 export");
        assert_eq!(buckets[0].net_t2_wh, 180, "200 import - 20 export");
    }

    #[test]
    fn electricity_meter_buckets_skips_an_hour_where_one_tariff_has_no_net_value() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("CREATE TABLE readings (type TEXT NOT NULL, timestamp INTEGER NOT NULL, value INTEGER NOT NULL);")
            .unwrap();
        let insert = |kind: &str, timestamp: i64, value: i64| {
            conn.execute("INSERT INTO readings (type, timestamp, value) VALUES (?1, ?2, ?3)", (kind, timestamp, value))
                .unwrap();
        };

        // Hour 0: both tariffs pair up fully -- should produce a bucket.
        insert("E-import-T1", 0, 100);
        insert("E-export-T1", 0, 10);
        insert("E-import-T2", 0, 200);
        insert("E-export-T2", 0, 20);

        // Hour SECONDS_PER_HOUR: T1 pairs fine, but E-export-T2 is missing
        // entirely for this hour, so net_series_sorted can't pair *any*
        // T2 reading here -- net_t2 ends up with no entry for this hour at
        // all, so the whole hour must be skipped, not just T2's half of it.
        insert("E-import-T1", SECONDS_PER_HOUR, 101);
        insert("E-export-T1", SECONDS_PER_HOUR, 11);
        insert("E-import-T2", SECONDS_PER_HOUR, 201);

        let buckets = electricity_meter_buckets(&conn, false).unwrap();

        assert_eq!(buckets.len(), 1, "the hour missing a net T2 value must be skipped");
        assert_eq!(buckets[0].bucket_start, 0);
    }

    #[test]
    fn quiet_only_suppresses_logging_never_changes_the_computed_data() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("CREATE TABLE readings (type TEXT NOT NULL, timestamp INTEGER NOT NULL, value INTEGER NOT NULL);")
            .unwrap();
        let insert = |kind: &str, timestamp: i64, value: i64| {
            conn.execute("INSERT INTO readings (type, timestamp, value) VALUES (?1, ?2, ?3)", (kind, timestamp, value))
                .unwrap();
        };

        // A P-import with no matching P-export -- the exact anomaly whose
        // *logging* quiet is meant to suppress. The resulting net series
        // must be identical either way; only whether it warns should differ.
        insert("P-import", 0, 100);
        insert("P-export", 0, 0);
        insert("P-import", 60, 200); // orphan: no matching P-export at 60

        let loud = net_series_sorted(&conn, "P-import", "P-export", false).unwrap();
        let quiet = net_series_sorted(&conn, "P-import", "P-export", true).unwrap();
        assert_eq!(loud, quiet);
        assert_eq!(loud, vec![(0, 100)], "the orphaned P-import must still be skipped, quiet or not");
    }

    #[test]
    fn dedup_last_per_timestamp_keeps_only_the_final_value_per_duplicate() {
        let series = vec![(0, 1), (5, 10), (5, 20), (5, 30), (6, 100)];
        assert_eq!(dedup_last_per_timestamp(series), vec![(0, 1), (5, 30), (6, 100)]);
    }

    #[test]
    fn net_series_sorted_handles_a_duplicate_timestamp_without_warning_worthy_mismatch() {
        // Reproduces a real production case: telegram timing jitter landed
        // two readings on the same labelled second (1784183542) -- import
        // duplicated with the same value, export duplicated with two
        // *different* values, exactly like the raw data behind the original
        // "no matching P-export" warnings. Before dedup_last_per_timestamp,
        // the second (duplicate) import row found the export HashMap
        // already drained by the first one and warned; after, both sides
        // collapse to their last value before pairing, so this nets
        // cleanly with no mismatch at all.
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("CREATE TABLE readings (type TEXT NOT NULL, timestamp INTEGER NOT NULL, value INTEGER NOT NULL);")
            .unwrap();
        let insert = |kind: &str, timestamp: i64, value: i64| {
            conn.execute("INSERT INTO readings (type, timestamp, value) VALUES (?1, ?2, ?3)", (kind, timestamp, value))
                .unwrap();
        };

        insert("P-import", 1, 0);
        insert("P-export", 1, 135);
        insert("P-import", 2, 0);
        insert("P-import", 2, 0); // duplicate telegram, same value
        insert("P-export", 2, 135);
        insert("P-export", 2, 133); // duplicate telegram, different value -- last one should win
        insert("P-import", 4, 0);
        insert("P-export", 4, 136);

        let net = net_series_sorted(&conn, "P-import", "P-export", false).unwrap();
        assert_eq!(net, vec![(1, -135), (2, -133), (4, -136)], "timestamp 2 should net using the last export value, once");
    }
}
