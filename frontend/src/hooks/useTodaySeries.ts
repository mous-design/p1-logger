import { useEffect, useState } from "react";
import type { Metric } from "../types";
import { useSeries } from "./useSeries";

// The aggregates behind these charts are rebuilt hourly by cron (at :05), so
// refreshing more often than this buys nothing -- 5 minutes keeps a tile at
// most that far behind an aggregate run, and rolls it over to the new day
// within 5 minutes of local midnight.
const REFRESH_MS = 5 * 60 * 1000;

function nowSeconds(): number {
  return Math.floor(Date.now() / 1000);
}

/** "Today" for a dashboard tile, kept current for a dashboard left open: the
 * anchor moves to the current time on every refresh, which both refetches
 * and rolls over at midnight. `loading` only covers the very first fetch --
 * a background refresh keeps showing the previous chart instead of blanking
 * the tile to "Laden…" every 5 minutes. */
export function useTodaySeries(metric: Metric) {
  const [anchor, setAnchor] = useState(nowSeconds);
  useEffect(() => {
    const id = setInterval(() => setAnchor(nowSeconds()), REFRESH_MS);
    return () => clearInterval(id);
  }, []);
  const { data, loading, error } = useSeries(metric, "day", anchor);
  return { data, loading: loading && data === null, error };
}
