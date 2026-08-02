//! Runs the actual compiled `p1-logger` binary end-to-end against a
//! committed multi-telegram fixture file (not a serial device -- exercises
//! the same file-source path `./run <file>` uses for manual testing). Reads
//! the raw day-file it produces and checks the parsed values directly,
//! rather than re-deriving this by hand against a scratch file each time.

use rusqlite::Connection;
use std::path::PathBuf;
use std::process::Command;

const FIXTURE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/sample_telegrams.txt");

// Both telegrams' own timestamp (0-0:1.0.0), 10s apart.
const TELEGRAM_A_TS: i64 = 1_767_603_600;
const TELEGRAM_B_TS: i64 = 1_767_603_610;
// The gas line's own timestamp (0-1:24.2.1) is deliberately a minute
// earlier than either telegram's, matching the real quirk documented in
// CLAUDE.md: the gas reading carries its own stamp, not the telegram's.
const GAS_TS: i64 = 1_767_603_540;

fn fresh_temp_dir(label: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("p1-logger-e2e-{label}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn reads_a_multi_telegram_file_and_writes_every_record() {
    let dir = fresh_temp_dir("basic");

    // A plain file source hits EOF and the binary exits on its own -- no
    // timeout/kill needed, unlike running against a real serial device.
    let output = Command::new(env!("CARGO_BIN_EXE_p1-logger"))
        .args([FIXTURE, "--data-dir", dir.to_str().unwrap(), "-q"])
        .output()
        .expect("failed to spawn p1-logger binary");
    assert!(output.status.success(), "stderr: {}", String::from_utf8_lossy(&output.stderr));

    let day_file = dir.join("2026-01-05.sqlite3");
    assert!(day_file.exists(), "expected a day-file for 2026-01-05");

    let conn = Connection::open(&day_file).unwrap();
    let get = |kind: &str, timestamp: i64| -> i64 {
        conn.query_row(
            "SELECT value FROM readings WHERE type = ?1 AND timestamp = ?2",
            (kind, timestamp),
            |row| row.get(0),
        )
        .unwrap_or_else(|err| panic!("missing {kind}@{timestamp}: {err}"))
    };

    // One value per telegram, proving both telegrams in the file were read.
    assert_eq!(get("P-import", TELEGRAM_A_TS), 500);
    assert_eq!(get("P-import", TELEGRAM_B_TS), 600);
    assert_eq!(get("E-import-T1", TELEGRAM_A_TS), 1_000_000);
    assert_eq!(get("E-import-T1", TELEGRAM_B_TS), 1_000_001);

    // The gas record must be filed under its own timestamp, not either
    // telegram's -- this is the exact behaviour db.rs's own doc comment
    // calls out as an accepted, deliberate edge case.
    assert_eq!(get("G-import", GAS_TS), 7_700_000);

    let total: i64 = conn.query_row("SELECT COUNT(*) FROM readings", [], |row| row.get(0)).unwrap();
    assert_eq!(total, 26, "13 records/telegram (per parser.rs) * 2 telegrams");

    std::fs::remove_dir_all(&dir).unwrap();
}
