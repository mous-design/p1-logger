import { useEffect, useState } from "react";
import { fetchElectricitySeries, fetchGasSeries, fetchPowerSeries } from "../api/series";
import type { Metric, Period, Series } from "../types";

interface UseSeriesResult {
  data: Series | null;
  loading: boolean;
  error: string | null;
}

const FETCHERS: Record<Metric, (period: Period, anchor: number) => Promise<Series>> = {
  power: fetchPowerSeries,
  electricity: fetchElectricitySeries,
  gas: fetchGasSeries,
};

export function useSeries(metric: Metric, period: Period, anchor: number): UseSeriesResult {
  const [data, setData] = useState<Series | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    setLoading(true);
    setError(null);
    FETCHERS[metric](period, anchor)
      .then((series) => {
        if (!cancelled) setData(series);
      })
      .catch((err: unknown) => {
        if (!cancelled) setError(err instanceof Error ? err.message : String(err));
      })
      .finally(() => {
        if (!cancelled) setLoading(false);
      });
    // Guards against a slower, stale request for a previous metric/period/anchor
    // overwriting the result of a newer one that resolves first.
    return () => {
      cancelled = true;
    };
  }, [metric, period, anchor]);

  return { data, loading, error };
}
