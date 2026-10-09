import type { ElectricitySeries, GasSeries, Metric, Period, PowerSeries, Series } from "../types";

// getTimezoneOffset() returns minutes to ADD to local time to reach UTC
// (e.g. -120 for CEST), the opposite sign convention from what the server
// wants (seconds to add to UTC to reach local time) -- hence the negation.
function localUtcOffsetSeconds(date: Date): number {
  return -date.getTimezoneOffset() * 60;
}

// Sampling the offset at the anchor's own instant breaks for year/5year (and
// occasionally month/week): the server applies ONE offset to the whole
// range, but a year always spans both DST transitions, so its own boundary
// (1 January) sits in wintertime (CET, +1h) even when the anchor is deep in
// summer (CEST, +2h) -- the mismatch overshoots the boundary by exactly that
// 1h delta, landing it in "31 december" instead of "1 januari" once the
// server's computed epoch gets displayed back in the browser's real,
// DST-aware local time. Sampling at the period's own start (computed with
// native Date arithmetic, which *is* DST-correct) instead of the anchor
// fixes every case except a month/week that itself straddles a transition --
// no single offset can get both of *that* range's own ends right, which is
// the irreducible case period.rs's own doc comment already accepts.
function localPeriodOffsetSeconds(period: Period, anchor: number): number {
  const d = new Date(anchor * 1000);
  switch (period) {
    case "day":
      return localUtcOffsetSeconds(new Date(d.getFullYear(), d.getMonth(), d.getDate()));
    case "week": {
      const daysSinceMonday = (d.getDay() + 6) % 7;
      return localUtcOffsetSeconds(new Date(d.getFullYear(), d.getMonth(), d.getDate() - daysSinceMonday));
    }
    case "month":
      return localUtcOffsetSeconds(new Date(d.getFullYear(), d.getMonth(), 1));
    case "year":
    case "5year":
      return localUtcOffsetSeconds(new Date(d.getFullYear(), 0, 1));
  }
}

async function fetchSeries<T extends Series>(metric: Metric, period: Period, anchor: number): Promise<T> {
  const utcOffsetSeconds = localPeriodOffsetSeconds(period, anchor);
  const url = `/api/series/${metric}?period=${period}&anchor=${anchor}&utc_offset_seconds=${utcOffsetSeconds}`;
  const response = await fetch(url);
  if (!response.ok) {
    const body = await response.json().catch(() => ({ error: response.statusText }));
    throw new Error(body.error ?? `request failed: ${response.status}`);
  }
  return response.json();
}

export function fetchPowerSeries(period: Period, anchor: number): Promise<PowerSeries> {
  return fetchSeries<PowerSeries>("power", period, anchor);
}

export function fetchElectricitySeries(period: Period, anchor: number): Promise<ElectricitySeries> {
  return fetchSeries<ElectricitySeries>("electricity", period, anchor);
}

export function fetchGasSeries(period: Period, anchor: number): Promise<GasSeries> {
  return fetchSeries<GasSeries>("gas", period, anchor);
}
