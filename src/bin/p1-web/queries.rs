use crate::period::Resolution;
use p1_logger::aggregate::{ElectricityMeterBucket, GasMeterBucket, PowerBucket};
use rusqlite::{Connection, OpenFlags, OptionalExtension};
use serde::Serialize;
use std::path::Path;

const SECONDS_PER_DAY: i64 = 86_400;
const SECONDS_PER_HOUR: i64 = 3600;

/// A cumulative meter register's hourly buckets for `[start, end)`, plus the
/// bucket of the hour just before `start`. Each bucket holds the register's
/// last reading *within* its hour, so that one's value is the register at
/// `start` itself -- the only correct baseline for "usage within this
/// range". Diffing against the range's own first bucket instead would
/// silently drop the first hour's usage. `None` if that hour has no data
/// (logger down, or the very start of the recordings).
pub struct MeterSeries<B> {
    pub baseline: Option<B>,
    pub buckets: Vec<B>,
}

/// Opens a fresh, short-lived read-only connection to `aggregates.sqlite3`.
/// `Ok(None)` only if the file definitely doesn't exist yet -- see
/// `main.rs`'s doc comment: that's a normal startup-ordering state, not an
/// error, so every query function here treats it as "no data yet" (empty
/// result). Any other open failure (permissions, a corrupt header, ...) is a
/// real error and propagates, so the HTTP layer logs it and answers 500
/// instead of quietly serving empty charts.
fn open_read_only(path: &Path) -> rusqlite::Result<Option<Connection>> {
    match Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY) {
        Ok(conn) => Ok(Some(conn)),
        Err(_) if matches!(path.try_exists(), Ok(false)) => Ok(None),
        Err(err) => Err(err),
    }
}

/// `resolution` picks the table, never a caller-supplied string -- avoids
/// building SQL from anything that traces back to the query string.
pub fn power(path: &Path, resolution: Resolution, start: i64, end: i64) -> rusqlite::Result<Vec<PowerBucket>> {
    let Some(conn) = open_read_only(path)? else {
        return Ok(Vec::new());
    };
    let table = match resolution {
        Resolution::Minute => "power_1min",
        Resolution::Day => "power_1day",
    };
    let mut stmt = conn.prepare(&format!(
        "SELECT bucket_start, min_w, avg_w, max_w FROM {table} WHERE bucket_start >= ?1 AND bucket_start < ?2 ORDER BY bucket_start"
    ))?;
    stmt.query_map((start, end), |row| {
        Ok(PowerBucket { bucket_start: row.get(0)?, min_w: row.get(1)?, avg_w: row.get(2)?, max_w: row.get(3)? })
    })?
    .collect()
}

/// `meter_electricity` is hourly-only (no minute/day split like power), so
/// unlike `power` this doesn't take a `Resolution` -- every `period` queries
/// the same table over its own `[start, end)` range (plus the baseline hour,
/// see `MeterSeries`).
pub fn electricity(path: &Path, start: i64, end: i64) -> rusqlite::Result<MeterSeries<ElectricityMeterBucket>> {
    let Some(conn) = open_read_only(path)? else {
        return Ok(MeterSeries { baseline: None, buckets: Vec::new() });
    };
    let mut stmt = conn.prepare(
        "SELECT bucket_start, net_t1_wh, net_t2_wh FROM meter_electricity \
         WHERE bucket_start >= ?1 AND bucket_start < ?2 ORDER BY bucket_start",
    )?;
    let mut buckets: Vec<ElectricityMeterBucket> = stmt
        .query_map((start - SECONDS_PER_HOUR, end), |row| {
            Ok(ElectricityMeterBucket { bucket_start: row.get(0)?, net_t1_wh: row.get(1)?, net_t2_wh: row.get(2)? })
        })?
        .collect::<rusqlite::Result<_>>()?;
    let baseline = buckets.first().is_some_and(|b| b.bucket_start < start).then(|| buckets.remove(0));
    Ok(MeterSeries { baseline, buckets })
}

/// Same hourly-only shape as `electricity` -- `meter_gas` has no separate
/// resolution table either.
pub fn gas(path: &Path, start: i64, end: i64) -> rusqlite::Result<MeterSeries<GasMeterBucket>> {
    let Some(conn) = open_read_only(path)? else {
        return Ok(MeterSeries { baseline: None, buckets: Vec::new() });
    };
    let mut stmt =
        conn.prepare("SELECT bucket_start, import_dm3 FROM meter_gas WHERE bucket_start >= ?1 AND bucket_start < ?2 ORDER BY bucket_start")?;
    let mut buckets: Vec<GasMeterBucket> = stmt
        .query_map((start - SECONDS_PER_HOUR, end), |row| Ok(GasMeterBucket { bucket_start: row.get(0)?, import_dm3: row.get(1)? }))?
        .collect::<rusqlite::Result<_>>()?;
    let baseline = buckets.first().is_some_and(|b| b.bucket_start < start).then(|| buckets.remove(0));
    Ok(MeterSeries { baseline, buckets })
}

/// Live snapshot for the "Tellerstanden" widget -- reads straight from
/// today's raw day-file (`p1-logger`'s own `readings` table), not the
/// aggregates. `power_w` is the one field that's netted (import minus
/// export) -- everything else is the raw register value exactly as it'd
/// read on the physical meter, since that's literally what this widget is
/// for: the actual tellerstanden, not a computed net. Every field is `None`
/// independently: the day-file might not exist yet (just after a
/// UTC-midnight rollover, before the first telegram lands), or a given
/// `type` might simply have no rows yet even though others do. Serialized
/// as-is for `/api/current` -- its fields already are the JSON contract.
#[derive(Serialize)]
pub struct CurrentReadings {
    /// The freshest timestamp actually used below, across whichever fields
    /// had data -- lets the frontend show "bijgewerkt Xs geleden" or grey
    /// out a stale reading, without the server having its own opinion on
    /// what counts as stale.
    pub at: Option<i64>,
    pub power_w: Option<i64>,
    pub import_t1_wh: Option<i64>,
    pub export_t1_wh: Option<i64>,
    pub import_t2_wh: Option<i64>,
    pub export_t2_wh: Option<i64>,
    pub gas_dm3: Option<i64>,
}

fn latest(conn: &Connection, kind: &str) -> rusqlite::Result<Option<(i64, i64)>> {
    conn.query_row("SELECT timestamp, value FROM readings WHERE type = ?1 ORDER BY timestamp DESC LIMIT 1", [kind], |row| {
        Ok((row.get(0)?, row.get(1)?))
    })
    .optional()
}

fn net(import: Option<(i64, i64)>, export: Option<(i64, i64)>) -> Option<i64> {
    match (import, export) {
        (Some((_, imp)), Some((_, exp))) => Some(imp - exp),
        _ => None,
    }
}

pub fn current(data_dir: &Path) -> rusqlite::Result<CurrentReadings> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system clock before 1970")
        .as_secs() as i64;
    let today = now.div_euclid(SECONDS_PER_DAY);
    let path = p1_logger::db::day_file_path(data_dir, today);

    let Some(conn) = open_read_only(&path)? else {
        return Ok(CurrentReadings {
            at: None,
            power_w: None,
            import_t1_wh: None,
            export_t1_wh: None,
            import_t2_wh: None,
            export_t2_wh: None,
            gas_dm3: None,
        });
    };

    let p_import = latest(&conn, "P-import")?;
    let p_export = latest(&conn, "P-export")?;
    let e_import_t1 = latest(&conn, "E-import-T1")?;
    let e_export_t1 = latest(&conn, "E-export-T1")?;
    let e_import_t2 = latest(&conn, "E-import-T2")?;
    let e_export_t2 = latest(&conn, "E-export-T2")?;
    let g_import = latest(&conn, "G-import")?;

    let at = [p_import, p_export, e_import_t1, e_export_t1, e_import_t2, e_export_t2, g_import]
        .into_iter()
        .flatten()
        .map(|(timestamp, _)| timestamp)
        .max();

    Ok(CurrentReadings {
        at,
        power_w: net(p_import, p_export),
        import_t1_wh: e_import_t1.map(|(_, value)| value),
        export_t1_wh: e_export_t1.map(|(_, value)| value),
        import_t2_wh: e_import_t2.map(|(_, value)| value),
        export_t2_wh: e_export_t2.map(|(_, value)| value),
        gas_dm3: g_import.map(|(_, value)| value),
    })
}
