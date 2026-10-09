import { useEffect, useState } from "react";
import { fetchCurrentReadings } from "../api/current";
import type { CurrentReadings } from "../types";

interface UseCurrentReadingsResult {
  data: CurrentReadings | null;
  error: string | null;
}

/** Polls `/api/current` on a fixed interval -- plain polling, not
 * websockets/SSE: this is a handful of bytes every few seconds on a LAN,
 * nowhere near where that complexity would pay for itself. */
export function useCurrentReadings(intervalMs: number): UseCurrentReadingsResult {
  const [data, setData] = useState<CurrentReadings | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    const poll = () => {
      fetchCurrentReadings()
        .then((readings) => {
          if (!cancelled) {
            setData(readings);
            setError(null);
          }
        })
        .catch((err: unknown) => {
          if (!cancelled) setError(err instanceof Error ? err.message : String(err));
        });
    };
    poll();
    const id = setInterval(poll, intervalMs);
    return () => {
      cancelled = true;
      clearInterval(id);
    };
  }, [intervalMs]);

  return { data, error };
}
