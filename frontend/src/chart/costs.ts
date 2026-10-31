import type { ElectricityPoint, GasPoint, TariffPeriod } from "../types";
import type { RegisterKey } from "./deltas";

/** The viewer's local calendar day of epoch-second `t`, as "YYYY-MM-DD" --
 * the same form as a tariff period's from/until, so plain string comparison
 * orders them correctly. */
function localDay(t: number): string {
  const d = new Date(t * 1000);
  const pad = (n: number) => String(n).padStart(2, "0");
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`;
}

/** The period covering the local day of `t`. Never missing: p1-web refuses
 * to start with periods that don't cover the whole timeline, so a miss here
 * is a real bug, not a configuration state to handle. */
function tariffAt(tariffs: TariffPeriod[], t: number): TariffPeriod {
  const day = localDay(t);
  const period = tariffs.find((p) => (p.from === null || p.from <= day) && (p.until === null || day <= p.until));
  if (!period) throw new Error(`no tariff period covers ${day}`);
  return period;
}

const REGISTER_PRICES = [
  ["import_t1_wh", "import_t1"],
  ["export_t1_wh", "export_t1"],
  ["import_t2_wh", "import_t2"],
  ["export_t2_wh", "export_t2"],
] as const;

/** € per register over a period -- null when any hour lacks a reading (see
 * ElectricityTotals), rather than a total that's silently too low. */
export interface ElectricityCosts {
  importT1Eur: number | null;
  exportT1Eur: number | null;
  importT2Eur: number | null;
  exportT2Eur: number | null;
}

// Priced hour by hour, not as one period total x one price: a period can
// span a tariff change, and each hour has to count at the price that held
// on its own day. Hour i's usage is its reading minus the one before it --
// the baseline for the first hour, exactly like electricityTotals, so the
// kWh and the € always cover the same hours.
export function electricityCosts(
  points: ElectricityPoint[],
  baseline: ElectricityPoint | undefined,
  tariffs: TariffPeriod[],
): ElectricityCosts {
  const eur: Record<RegisterKey, number | null> = { import_t1_wh: 0, export_t1_wh: 0, import_t2_wh: 0, export_t2_wh: 0 };
  points.forEach((p, i) => {
    const prev = i === 0 ? baseline : points[i - 1];
    if (!prev) return;
    const prices = tariffAt(tariffs, p.t).electricity;
    for (const [key, price] of REGISTER_PRICES) {
      const sum = eur[key];
      const from = prev[key];
      const to = p[key];
      eur[key] = sum === null || from === null || to === null ? null : sum + ((to - from) / 1000) * prices[price];
    }
  });
  return {
    importT1Eur: eur.import_t1_wh,
    exportT1Eur: eur.export_t1_wh,
    importT2Eur: eur.import_t2_wh,
    exportT2Eur: eur.export_t2_wh,
  };
}

// Same hour-by-hour pricing as electricityCosts. The gas register is never
// null, so this always has a value.
export function gasCost(points: GasPoint[], baseline: GasPoint | undefined, tariffs: TariffPeriod[]): number {
  let eur = 0;
  points.forEach((p, i) => {
    const prev = i === 0 ? baseline : points[i - 1];
    if (!prev) return;
    eur += ((p.import_dm3 - prev.import_dm3) / 1000) * tariffAt(tariffs, p.t).gas;
  });
  return eur;
}
