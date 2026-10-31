import { useEffect, useState } from "react";
import { fetchTariffs } from "../api/tariffs";
import type { TariffPeriod } from "../types";

// Tariffs only change when p1-web restarts with a new config, so one fetch
// per page load is enough -- shared by every chart that shows prices.
let tariffsPromise: Promise<TariffPeriod[] | null> | null = null;

/** `null` while loading, when none are configured, or when the fetch
 * failed -- in every case the charts simply show no euro amounts, since
 * prices are an extra on top of the readings, not something to block on. */
export function useTariffs(): TariffPeriod[] | null {
  const [tariffs, setTariffs] = useState<TariffPeriod[] | null>(null);
  useEffect(() => {
    let cancelled = false;
    tariffsPromise ??= fetchTariffs();
    tariffsPromise
      .then((result) => {
        if (!cancelled) setTariffs(result);
      })
      .catch((err: unknown) => {
        console.error("failed to load tariffs:", err);
        tariffsPromise = null;
      });
    return () => {
      cancelled = true;
    };
  }, []);
  return tariffs;
}
