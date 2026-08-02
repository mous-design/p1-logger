use crate::parser::{Record, Telegram};
use rusqlite::Connection;
use std::path::{Path, PathBuf};

const SECONDS_PER_DAY: i64 = 86_400;

/// Raw-reading store, one SQLite file per UTC calendar day. A day boundary
/// is a fixed-size epoch division (`timestamp.div_euclid(86_400)`), not a
/// local-time midnight -- that sidesteps the exact DST ambiguity the
/// parser's timestamp handling already had to deal with.
///
/// Pruning old day-files and building aggregates is out of scope here by
/// design: that's the future `p1-aggregate` binary's job, not the writer's.
pub struct RawStore {
    dir: PathBuf,
    open: Option<(i64, Connection)>,
}

impl RawStore {
    pub fn new(dir: PathBuf) -> Self {
        Self { dir, open: None }
    }

    /// Inserts every record from one telegram in a single transaction, so a
    /// crash mid-batch loses the whole telegram rather than part of it.
    /// Bucketed by the telegram's own timestamp (not a record's), since
    /// OBIS-code order isn't spec-guaranteed and the gas reading can carry
    /// a slightly different timestamp of its own -- so right around
    /// midnight, a gas record may land in the "wrong" day-file by a few
    /// seconds. Accepted trade-off: splitting a telegram across two day
    /// connections to avoid that would break the single-transaction
    /// guarantee for a once-a-day, few-second edge case.
    pub fn insert(&mut self, telegram: &Telegram) -> rusqlite::Result<()> {
        if telegram.records.is_empty() {
            return Ok(());
        }

        let day = telegram.timestamp.div_euclid(SECONDS_PER_DAY);
        let is_correct_day_open = match &self.open {
            Some((open_day, _)) => *open_day == day,
            None => false,
        };
        if !is_correct_day_open {
            self.open = Some((day, open_day_file(&self.dir, day)?));
        }
        let (_, conn) = self.open.as_mut().unwrap();

        insert_records(conn, &telegram.records)
    }
}

fn insert_records(conn: &mut Connection, records: &[Record]) -> rusqlite::Result<()> {
    let tx = conn.transaction()?;
    {
        let mut stmt =
            tx.prepare_cached("INSERT INTO readings (type, timestamp, value) VALUES (?1, ?2, ?3)")?;
        for record in records {
            stmt.execute((record.kind, record.timestamp, record.value))?;
        }
    }
    tx.commit()
}

fn open_day_file(dir: &Path, day: i64) -> rusqlite::Result<Connection> {
    let conn = Connection::open(day_file_path(dir, day))?;
    conn.pragma_update(None, "journal_mode", "WAL")?;
    // WAL's own default sync setting is safe under crashes/power loss
    // without fsync-ing on every single commit -- SQLite's default of
    // FULL does fsync every commit even in WAL mode, which is far more
    // fsync pressure than needed and was stalling the writer thread.
    conn.pragma_update(None, "synchronous", "NORMAL")?;
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS readings (
            type      TEXT    NOT NULL,
            timestamp INTEGER NOT NULL,
            value     INTEGER NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_readings_type_timestamp
            ON readings (type, timestamp);",
    )?;
    Ok(conn)
}

pub fn day_file_path(dir: &Path, day: i64) -> PathBuf {
    let date = chrono::DateTime::from_timestamp(day * SECONDS_PER_DAY, 0)
        .expect("day number in representable range")
        .format("%Y-%m-%d");
    dir.join(format!("{date}.sqlite3"))
}

/// The reverse of `day_file_path`: reads the `YYYY-MM-DD` date out of a raw
/// day-file's name and returns its UTC midnight as an epoch timestamp
/// (`day_start`; the day's range is `[day_start, day_start + 86_400)`).
/// `None` for anything that isn't one of our own day-files (e.g.
/// `aggregates.sqlite3`), so callers can filter a directory listing with it.
pub fn parse_day_file_name(path: &Path) -> Option<i64> {
    // WAL mode leaves `<name>.sqlite3-wal`/`-shm` sidecar files next to the
    // real database. Their `file_stem()` is still the plain date (stem-splitting
    // only looks at the last '.'), so the extension must be checked too --
    // otherwise those sidecars get scanned in as if they were day-files.
    if path.extension().and_then(|ext| ext.to_str()) != Some("sqlite3") {
        return None;
    }
    let stem = path.file_stem()?.to_str()?;
    let date = chrono::NaiveDate::parse_from_str(stem, "%Y-%m-%d").ok()?;
    Some(date.and_hms_opt(0, 0, 0)?.and_utc().timestamp())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::Record;

    fn telegram_at(timestamp: i64, kind: &'static str, value: i64) -> Telegram {
        Telegram {
            timestamp,
            records: vec![Record { kind, timestamp, value }],
        }
    }

    #[test]
    fn parse_day_file_name_round_trips_with_day_file_path() {
        let dir = Path::new("data");
        let path = day_file_path(dir, 0); // epoch day 0 = 1970-01-01
        assert_eq!(parse_day_file_name(&path), Some(0));

        // 2026-07-12 14:31:15 UTC, same day used throughout the other tests
        let day = 1_783_866_675_i64.div_euclid(SECONDS_PER_DAY);
        let path = day_file_path(dir, day);
        assert_eq!(parse_day_file_name(&path), Some(day * SECONDS_PER_DAY));
    }

    #[test]
    fn parse_day_file_name_rejects_non_day_files() {
        assert_eq!(parse_day_file_name(Path::new("data/aggregates.sqlite3")), None);
    }

    #[test]
    fn parse_day_file_name_rejects_wal_and_shm_sidecar_files() {
        assert_eq!(parse_day_file_name(Path::new("data/2026-07-31.sqlite3-wal")), None);
        assert_eq!(parse_day_file_name(Path::new("data/2026-07-31.sqlite3-shm")), None);
    }

    #[test]
    fn inserts_land_in_a_day_file_named_by_date() {
        let dir = std::env::temp_dir().join(format!("p1-logger-db-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();

        let mut store = RawStore::new(dir.clone());
        // 2026-07-12 14:31:15 UTC
        store.insert(&telegram_at(1_783_866_675, "P-import", 220)).unwrap();

        let expected = dir.join("2026-07-12.sqlite3");
        assert!(expected.exists(), "expected {expected:?} to exist");

        let conn = Connection::open(&expected).unwrap();
        let (kind, timestamp, value): (String, i64, i64) = conn
            .query_row("SELECT type, timestamp, value FROM readings", [], |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?))
            })
            .unwrap();
        assert_eq!((kind.as_str(), timestamp, value), ("P-import", 1_783_866_675, 220));

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn rotates_to_a_new_file_when_the_day_changes() {
        let dir = std::env::temp_dir().join(format!("p1-logger-db-test-rotate-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();

        let mut store = RawStore::new(dir.clone());
        store.insert(&telegram_at(1_783_866_675, "P-import", 1)).unwrap(); // 2026-07-12
        store.insert(&telegram_at(1_783_866_675 + SECONDS_PER_DAY, "P-import", 2)).unwrap(); // 2026-07-13

        assert!(dir.join("2026-07-12.sqlite3").exists());
        assert!(dir.join("2026-07-13.sqlite3").exists());

        std::fs::remove_dir_all(&dir).unwrap();
    }
}
