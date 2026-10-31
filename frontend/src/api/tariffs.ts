import type { TariffPeriod } from "../types";

/** `null` when p1-web has no tariffs configured. */
export async function fetchTariffs(): Promise<TariffPeriod[] | null> {
  const response = await fetch("/api/tariffs");
  if (!response.ok) {
    const body = await response.json().catch(() => ({ error: response.statusText }));
    throw new Error(body.error ?? `request failed: ${response.status}`);
  }
  const body: { tariffs: TariffPeriod[] | null } = await response.json();
  return body.tariffs;
}
