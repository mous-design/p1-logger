use crate::period::{self, Period};
use crate::queries;
use p1_logger::aggregate::{ElectricityMeterBucket, GasMeterBucket, PowerBucket};
use serde::Serialize;
use std::io::Cursor;
use std::path::Path;
use tiny_http::{Header, Response};

#[derive(Serialize)]
struct PowerPoint {
    t: i64,
    min_w: i64,
    avg_w: i64,
    max_w: i64,
}

#[derive(Serialize)]
struct ElectricityPoint {
    t: i64,
    net_t1_wh: i64,
    net_t2_wh: i64,
}

#[derive(Serialize)]
struct GasPoint {
    t: i64,
    import_dm3: i64,
}

#[derive(Serialize)]
struct SeriesResponse<P> {
    metric: &'static str,
    period: &'static str,
    start: i64,
    end: i64,
    /// Only for the cumulative meter registers (electricity, gas) -- see
    /// `queries::MeterSeries`: the register at `start`, outside `points` on
    /// purpose so it can't be mistaken for a plottable bucket. Absent for
    /// power, which has no such thing, and when that hour has no data.
    #[serde(skip_serializing_if = "Option::is_none")]
    baseline: Option<P>,
    points: Vec<P>,
}

#[derive(Serialize)]
struct ErrorResponse<'a> {
    error: &'a str,
}

/// Handles `GET /api/series/<metric>?period=...&anchor=...` and
/// `GET /api/current`. `None` if `path` isn't under `/api/` at all -- the
/// caller tries the next candidate handler (static assets) for that case.
///
/// One match on up to 4 path segments (deepest known route, `/api/series/<metric>`,
/// is 3 -- the 4th slot is the "anything left over?" sentinel). Every `api/...`
/// path that doesn't match a known route still resolves to `Some(404)` here
/// rather than `None`, so a typo'd or unknown API path can't leak through to
/// the asset handler's SPA fallback and get served `index.html`.
pub fn handle(data_dir: &Path, aggregates_path: &Path, path: &str, query: &str) -> Option<Response<Cursor<Vec<u8>>>> {
    let mut segments = path.trim_start_matches('/').split('/');
    match (segments.next(), segments.next(), segments.next(), segments.next()) {
        (Some("api"), Some("current"), None, None) => Some(handle_current(data_dir)),
        (Some("api"), Some("series"), Some(metric), None) => Some(handle_series(aggregates_path, metric, query)),
        (Some("api"), ..) => Some(error_response(404, "not found")),
        _ => None,
    }
}

fn get_series_params(query: &str) -> Result<(Period, i64, i64), &'static str> {
    let params = parse_query(query);
    let Some(period) = params.get("period").copied().and_then(Period::parse) else {
        return Err("unknown or missing period");
    };
    let Some(anchor) = params.get("anchor").and_then(|s| s.parse::<i64>().ok()) else {
        return Err("missing or non-numeric anchor");
    };
    // Defaults to UTC (0) when absent, e.g. for a plain curl check -- the
    // browser client always sends its real local offset (see
    // frontend/src/api/series.ts), since that's what makes "day" mean the
    // viewer's calendar day rather than the UTC one.
    let utc_offset_seconds = params.get("utc_offset_seconds").and_then(|s| s.parse::<i64>().ok()).unwrap_or(0);
    let Some((start, end)) = period::range(period, anchor, utc_offset_seconds) else {
        return Err("anchor or utc_offset_seconds out of range");
    };
    Ok((period, start, end))
}

fn handle_series(aggregates_path: &Path, metric: &str, query: &str) -> Response<Cursor<Vec<u8>>> {
    let params = get_series_params(query);
    match (metric, params) {
        ("power", Ok((period, start, end))) => {
            let result = queries::power(aggregates_path, period.resolution(), start, end);
            series_response("power", period, start, end, result.map(|buckets| (None, buckets.into_iter().map(power_point).collect())))
        }
        ("electricity", Ok((period, start, end))) => {
            let result = queries::electricity(aggregates_path, start, end);
            let result = result.map(|s| (s.baseline.map(electricity_point), s.buckets.into_iter().map(electricity_point).collect()));
            series_response("electricity", period, start, end, result)
        }
        ("gas", Ok((period, start, end))) => {
            let result = queries::gas(aggregates_path, start, end);
            series_response("gas", period, start, end, result.map(|s| (s.baseline.map(gas_point), s.buckets.into_iter().map(gas_point).collect())))
        }
        ("power" | "electricity" | "gas", Err(err)) => error_response(400, err),
        _ => error_response(404, "not found"),
    }
}

/// `result` is `(baseline, points)`, already mapped to the response's point
/// type -- the one place a query failure turns into a logged 500.
fn series_response<P: Serialize>(
    metric: &'static str,
    period: Period,
    start: i64,
    end: i64,
    result: rusqlite::Result<(Option<P>, Vec<P>)>,
) -> Response<Cursor<Vec<u8>>> {
    match result {
        Ok((baseline, points)) => json_response(SeriesResponse { metric, period: period.as_str(), start, end, baseline, points }),
        Err(err) => {
            log::error!("{metric} query failed: {err}");
            error_response(500, "query failed")
        }
    }
}

fn power_point(b: PowerBucket) -> PowerPoint {
    PowerPoint { t: b.bucket_start, min_w: b.min_w, avg_w: b.avg_w, max_w: b.max_w }
}

fn electricity_point(b: ElectricityMeterBucket) -> ElectricityPoint {
    ElectricityPoint { t: b.bucket_start, net_t1_wh: b.net_t1_wh, net_t2_wh: b.net_t2_wh }
}

fn gas_point(b: GasMeterBucket) -> GasPoint {
    GasPoint { t: b.bucket_start, import_dm3: b.import_dm3 }
}

fn handle_current(data_dir: &Path) -> Response<Cursor<Vec<u8>>> {
    match queries::current(data_dir) {
        Ok(readings) => json_response(readings),
        Err(err) => {
            log::error!("current query failed: {err}");
            error_response(500, "query failed")
        }
    }
}

/// Simple query-string parser. Since all values are either numeric or enum, we 
/// don't need RFC 3986 percent-decoding. Mind you: should that change, start 
/// using a proper decoder with correct UTF-8 handling.
fn parse_query(query: &str) -> std::collections::HashMap<&str, &str> {
    query
        .split('&')
        .filter_map(|pair| pair.split_once('='))
        .filter(|(key, _)| !key.is_empty())
        .collect()
}

fn json_response(body: impl Serialize) -> Response<Cursor<Vec<u8>>> {
    let bytes = serde_json::to_vec(&body).expect("response structs always serialize");
    let header = Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..]).expect("static header is valid");
    Response::from_data(bytes).with_header(header)
}

fn error_response(status: u16, message: &str) -> Response<Cursor<Vec<u8>>> {
    json_response(ErrorResponse { error: message }).with_status_code(status)
}
