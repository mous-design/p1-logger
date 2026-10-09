import type { Metric, Period } from "./types";

const METRICS: readonly Metric[] = ["power", "electricity", "gas"];
const PERIODS: readonly Period[] = ["day", "week", "month", "year", "5year"];

export type Route = { kind: "dashboard" } | { kind: "detail"; metric: Metric };

/** Parses the current path into a route -- used both for the initial page
 * load (deep link / reload) and for popstate (browser back/forward).
 * Anything unrecognized falls back to the dashboard rather than erroring,
 * since a stale or hand-typed URL shouldn't strand the user on a blank
 * page. */
export function parseRoute(pathname: string): Route {
  const metric = pathname.match(/^\/detail\/([a-z0-9]+)\/?$/)?.[1];
  if (metric && (METRICS as readonly string[]).includes(metric)) {
    return { kind: "detail", metric: metric as Metric };
  }
  return { kind: "dashboard" };
}

export function detailPath(metric: Metric): string {
  return `/detail/${metric}`;
}

/** Reads period/anchor out of the URL's query string (e.g. right after a
 * deep link or a reload) -- `null` for either half means "not present",
 * letting the caller fall back to its own default independently rather
 * than requiring both or neither. */
export function parsePeriodAnchor(search: string): { period: Period | null; anchor: number | null } {
  const params = new URLSearchParams(search);
  const periodParam = params.get("period");
  const period = periodParam && (PERIODS as readonly string[]).includes(periodParam) ? (periodParam as Period) : null;
  const anchorParam = params.get("anchor");
  const anchorValue = anchorParam !== null ? Number(anchorParam) : NaN;
  const anchor = Number.isFinite(anchorValue) ? anchorValue : null;
  return { period, anchor };
}
