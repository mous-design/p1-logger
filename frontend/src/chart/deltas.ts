import type { ElectricityPoint, GasPoint } from "../types";

// combined_wh (T1+T2) is derived here, not re-derived in every consumer --
// both the Elektra detail chart and its dashboard tile need it.
export interface ElectricityDeltaPoint extends ElectricityPoint {
  combined_wh: number;
}

// net_t1_wh/net_t2_wh (and import_dm3) are cumulative net registers (running
// totals since the meter's install), not per-bucket deltas -- plotted as-is,
// a chart reads as "total so far today" instead of answering the useful
// question: how much happened *in* each bucket. Diffing consecutive points
// turns the cumulative register into that per-bucket delta, so an idle
// bucket reads as 0 rather than "whatever the running total happened to be".
//
// Each point is the register's last reading *within* its hour, so the first
// bucket's own usage needs the register at the period's start: `baseline`,
// the bucket of the hour before (sent separately by the API). Without one
// (that hour has no data), the first delta is 0 and the totals start at the
// first point -- the one case where a period's first hour can't be counted.

export function electricityDeltas(points: ElectricityPoint[], baseline: ElectricityPoint | undefined): ElectricityDeltaPoint[] {
  return points.map((p, i) => {
    const prev = i === 0 ? baseline : points[i - 1];
    if (!prev) return { t: p.t, net_t1_wh: 0, net_t2_wh: 0, combined_wh: 0 };
    const net_t1_wh = p.net_t1_wh - prev.net_t1_wh;
    const net_t2_wh = p.net_t2_wh - prev.net_t2_wh;
    return { t: p.t, net_t1_wh, net_t2_wh, combined_wh: net_t1_wh + net_t2_wh };
  });
}

// The period's net total: last reading minus the register at the period's
// start -- equal to the sum of electricityDeltas, first bucket included.
export function electricityNetTotals(points: ElectricityPoint[], baseline: ElectricityPoint | undefined): { t1Wh: number; t2Wh: number } {
  const first = baseline ?? points[0];
  const last = points[points.length - 1];
  if (!first || !last) return { t1Wh: 0, t2Wh: 0 };
  return { t1Wh: last.net_t1_wh - first.net_t1_wh, t2Wh: last.net_t2_wh - first.net_t2_wh };
}

export function gasDeltas(points: GasPoint[], baseline: GasPoint | undefined): GasPoint[] {
  return points.map((p, i) => {
    const prev = i === 0 ? baseline : points[i - 1];
    return { t: p.t, import_dm3: prev ? p.import_dm3 - prev.import_dm3 : 0 };
  });
}

// Same reasoning as electricityNetTotals.
export function gasNetTotal(points: GasPoint[], baseline: GasPoint | undefined): number {
  const first = baseline ?? points[0];
  const last = points[points.length - 1];
  if (!first || !last) return 0;
  return last.import_dm3 - first.import_dm3;
}
