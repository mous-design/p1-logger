use p1_logger::{aggregate, db};
use rusqlite::Connection;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

const SECONDS_PER_DAY: i64 = 86_400;

/// Scans `data_dir` for day-files, computes and writes each completed day's
/// aggregates into `aggregates_conn`, archives what it can into
/// `processed/`, and (if `retention_days` is set) prunes archived day-files
/// past the retention window.
pub fn run(data_dir: PathBuf, retention_days: Option<u32>, mut aggregates_conn: Connection) -> ExitCode {
    let today_start = current_utc_day_start();
    let processed_dir = data_dir.join("processed");
    std::fs::create_dir_all(&processed_dir).expect("failed to create processed dir");

    let mut day_files = collect_day_files(&data_dir).expect("failed to scan data-dir");
    day_files.sort_unstable_by_key(|(day_start, _)| *day_start);

    for (day_start, path) in &day_files {
        match process_day(&mut aggregates_conn, &processed_dir, today_start, *day_start, path) {
            Ok(archived) if archived => log::info!("aggregated and archived {}", path.display()),
            Ok(_) => log::info!("aggregated {} (today, not archived)", path.display()),
            // A bad day shouldn't block every day after it forever -- log
            // and move on, the file stays put and gets retried next run.
            Err(err) => log::error!("failed to process {}: {err}", path.display()),
        }
    }

    if let Some(retention_days) = retention_days {
        let cutoff_start = today_start - retention_days as i64 * SECONDS_PER_DAY;
        prune_old_archives(&processed_dir, cutoff_start);
    }

    ExitCode::SUCCESS
}

/// Deletes archived raw day-files older than the retention window. The raw
/// per-second data is ~56MB/day (see CLAUDE.md) -- fine for a while, but not
/// forever on a Pi's SD card, so this is opt-in via `--retention-days`
/// rather than an unconditional default. Only ever touches `processed/`:
/// today's still-growing file, and any not-yet-archived backlog, are never
/// in scope here regardless of how old their day_start is.
fn prune_old_archives(processed_dir: &Path, cutoff_start: i64) {
    let entries = match std::fs::read_dir(processed_dir) {
        Ok(entries) => entries,
        Err(err) => {
            log::error!("failed to scan {} for pruning: {err}", processed_dir.display());
            return;
        }
    };
    for entry in entries {
        let path = match entry {
            Ok(entry) => entry.path(),
            Err(err) => {
                log::error!("failed to read a directory entry while pruning: {err}");
                continue;
            }
        };
        let Some(day_start) = db::parse_day_file_name(&path) else {
            continue;
        };
        if day_start < cutoff_start {
            match std::fs::remove_file(&path) {
                Ok(()) => log::info!("pruned {} (older than retention window)", path.display()),
                Err(err) => log::error!("failed to prune {}: {err}", path.display()),
            }
        }
    }
}

/// Computes and writes one day's aggregates, then archives the raw file
/// unless it's still today's (still being written by the logger). Returns
/// whether it was archived.
///
/// Today's file gets recomputed and rewritten every hour, so its anomaly
/// warnings are suppressed (`quiet`) until the one run where it's no longer
/// today -- that final, pre-archive computation is the only one where every
/// real anomaly gets logged, exactly once. See `aggregate::write_day`'s doc
/// comment for the full reasoning.
fn process_day(
    aggregates_conn: &mut Connection,
    processed_dir: &Path,
    today_start: i64,
    day_start: i64,
    path: &Path,
) -> Result<bool, Box<dyn std::error::Error>> {
    let is_today = day_start >= today_start;

    let raw_conn = Connection::open(path)?;
    let data = aggregate::compute_day(&raw_conn, is_today)?;
    drop(raw_conn);

    aggregate::write_day(aggregates_conn, day_start, day_start + SECONDS_PER_DAY, &data, is_today)?;

    if is_today {
        return Ok(false);
    }
    let file_name = path.file_name().ok_or("day-file path has no filename")?;
    std::fs::rename(path, processed_dir.join(file_name))?;
    Ok(true)
}

fn collect_day_files(dir: &Path) -> std::io::Result<Vec<(i64, PathBuf)>> {
    let mut files = Vec::new();
    for entry in std::fs::read_dir(dir)? {
        let path = entry?.path();
        if let Some(day_start) = db::parse_day_file_name(&path) {
            files.push((day_start, path));
        }
    }
    Ok(files)
}

fn current_utc_day_start() -> i64 {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system clock before 1970")
        .as_secs() as i64;
    now.div_euclid(SECONDS_PER_DAY) * SECONDS_PER_DAY
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prune_old_archives_deletes_only_files_strictly_older_than_cutoff() {
        let dir = std::env::temp_dir().join(format!("p1-aggregate-prune-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        let old = db::day_file_path(&dir, 0); // 1970-01-01
        let at_cutoff = db::day_file_path(&dir, 10); // day 10
        let newer = db::day_file_path(&dir, 20); // day 20
        for path in [&old, &at_cutoff, &newer] {
            std::fs::write(path, b"").unwrap();
        }

        prune_old_archives(&dir, 10 * SECONDS_PER_DAY);

        assert!(!old.exists(), "day 0 is before the cutoff, should be pruned");
        assert!(at_cutoff.exists(), "day 10 is exactly at the cutoff, should be kept");
        assert!(newer.exists(), "day 20 is after the cutoff, should be kept");

        std::fs::remove_dir_all(&dir).unwrap();
    }
}
