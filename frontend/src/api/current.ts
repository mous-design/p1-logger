import type { CurrentReadings } from "../types";

export async function fetchCurrentReadings(): Promise<CurrentReadings> {
  const response = await fetch("/api/current");
  if (!response.ok) {
    const body = await response.json().catch(() => ({ error: response.statusText }));
    throw new Error(body.error ?? `request failed: ${response.status}`);
  }
  return response.json();
}
