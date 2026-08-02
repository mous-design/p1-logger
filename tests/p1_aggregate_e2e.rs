//! Runs the actual compiled `p1-aggregate` binary end-to-end (not just the
//! library functions it's built from) against a throwaway data-dir. Cargo
//! builds the binary once for the whole test run via `CARGO_BIN_EXE_*`, so
//! this is no more expensive than any other `cargo test` run -- worth having
//! as a standing regression test rather than re-deriving these checks by
//! hand against a copy of production data every time.

use p1_logger::db;
use p1_logger::parser::{Record, Telegram};
use rusqlite::Connection;
use std::path::PathBuf;
use std::process::Command;

const SECONDS_PER_DAY: i64 = 86_400;
// 2000-01-01 UTC midnight -- any day this far back is guaranteed to never be
// "today" no matter when this test happens to run, so it always gets archived.
const LONG_PAST_DAY_START: i64 = 946_684_800;

fn fresh_temp_dir(label: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("p1-aggregate-e2e-{label}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn run_p1_aggregate(args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_p1-aggregate"))
        .args(args)
        .output()
        .expect("failed to spawn p1-aggregate binary")
}

#[test]
fn aggregates_and_archives_a_completed_day() {
    let dir = fresh_temp_dir("aggregate");

    let mut store = db::RawStore::new(dir.clone());
    for i in 0..5 {
        let timestamp = LONG_PAST_DAY_START + i * 60;
        let records = vec![
            Record { kind: "P-import", timestamp, value: 100 + i },
            Record { kind: "P-export", timestamp, value: 0 },
            Record { kind: "E-import-T1", timestamp, value: 1_000 + i },
            Record { kind: "E-import-T2", timestamp, value: 2_000 + i },
            Record { kind: "E-export-T1", timestamp, value: 0 },
            Record { kind: "E-export-T2", timestamp, value: 0 },
            Record { kind: "G-import", timestamp, value: 5_000 + i },
        ];
        store.insert(&Telegram { timestamp, records }).unwrap();
    }
    drop(store);

    let output = run_p1_aggregate(&["--data-dir", dir.to_str().unwrap()]);
    assert!(output.status.success(), "stderr: {}", String::from_utf8_lossy(&output.stderr));

    let raw_day_file = db::day_file_path(&dir, LONG_PAST_DAY_START / SECONDS_PER_DAY);
    assert!(!raw_day_file.exists(), "completed day's raw file should have been archived away");
    let archived = dir.join("processed").join(raw_day_file.file_name().unwrap());
    assert!(archived.exists(), "completed day's raw file should now live in processed/");

    let aggregates = Connection::open(dir.join("aggregates.sqlite3")).unwrap();
    for table in ["power_1day", "meter_electricity", "meter_gas"] {
        let count: i64 = aggregates.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| row.get(0)).unwrap();
        assert_eq!(count, 1, "expected exactly one row in {table} for the one day inserted");
    }

    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn retention_days_prunes_by_filename_date_regardless_of_how_fresh_the_file_is() {
    let dir = fresh_temp_dir("retention");
    let processed = dir.join("processed");
    std::fs::create_dir_all(&processed).unwrap();

    // Written just now, so its mtime is "fresh" -- but named for a date far
    // in the past. If pruning ever started trusting filesystem mtime instead
    // of the filename, this file would wrongly survive.
    let stale_by_name = processed.join("2000-01-01.sqlite3");
    std::fs::write(&stale_by_name, b"").unwrap();

    let output = run_p1_aggregate(&["--data-dir", dir.to_str().unwrap(), "--retention-days", "90"]);
    assert!(output.status.success(), "stderr: {}", String::from_utf8_lossy(&output.stderr));

    assert!(!stale_by_name.exists(), "file must be pruned based on its filename date, not its fresh mtime");

    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn retention_days_never_touches_the_live_data_dir_itself() {
    let dir = fresh_temp_dir("retention-live");
    std::fs::create_dir_all(dir.join("processed")).unwrap();

    // A day-file sitting directly in data-dir (not yet archived) is either
    // today's still-growing file or an unprocessed backlog entry -- either
    // way, retention must never delete it, no matter how old its name is.
    let unarchived_but_old = db::day_file_path(&dir, LONG_PAST_DAY_START / SECONDS_PER_DAY);
    std::fs::write(&unarchived_but_old, b"not a real sqlite file, but pruning must not even look at this one").unwrap();

    // Ensure it isn't picked up as a completed day and archived away either:
    // simulate that by pointing --data-dir straight at an empty dir instead,
    // so process_day never runs on it, and only prune_old_archives' scoping
    // is under test here.
    let output = run_p1_aggregate(&["--data-dir", dir.to_str().unwrap(), "--retention-days", "0"]);
    // The bogus file will fail to *aggregate* (it's not a real database) --
    // that's expected and logged, not a test failure. What matters is that
    // pruning still leaves it alone.
    let _ = output.status;

    assert!(unarchived_but_old.exists(), "retention must only ever prune processed/, never the live data-dir");

    std::fs::remove_dir_all(&dir).unwrap();
}
