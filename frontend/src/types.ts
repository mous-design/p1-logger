export type Period = "day" | "week" | "month" | "year" | "5year";

export type Metric = "power" | "electricity" | "gas";

/** A dashboard tile: one per metric, plus the live "Tellerstanden" overview. */
export type WidgetId = "overview" | Metric;

export interface Widget {
  id: WidgetId;
  label: string;
}

export interface PowerPoint {
  t: number;
  min_w: number;
  avg_w: number;
  max_w: number;
}

export interface PowerSeries {
  metric: "power";
  period: Period;
  start: number;
  end: number;
  points: PowerPoint[];
}

export interface ElectricityPoint {
  t: number;
  net_t1_wh: number;
  net_t2_wh: number;
}

export interface ElectricitySeries {
  metric: "electricity";
  period: Period;
  start: number;
  end: number;
  /** The register at `start` (the bucket of the hour before it); absent if
   * that hour has no data -- see deltas.ts for why this isn't just points[0]. */
  baseline?: ElectricityPoint;
  points: ElectricityPoint[];
}

export interface GasPoint {
  t: number;
  import_dm3: number;
}

export interface GasSeries {
  metric: "gas";
  period: Period;
  start: number;
  end: number;
  /** Same as ElectricitySeries.baseline. */
  baseline?: GasPoint;
  points: GasPoint[];
}

export type Series = PowerSeries | ElectricitySeries | GasSeries;

/** Live snapshot from `/api/current` -- read straight from today's raw
 * day-file, not the aggregates. `power_w` is netted (import minus export);
 * everything else is the raw register value exactly as it'd read on the
 * physical meter -- the "Tellerstanden" widget shows literal tellerstanden,
 * not a computed net. Every field is independently nullable: no data yet
 * (just after a UTC-midnight rollover) leaves them all null, but a single
 * missing reading type only nulls that one field. */
export interface CurrentReadings {
  at: number | null;
  power_w: number | null;
  import_t1_wh: number | null;
  export_t1_wh: number | null;
  import_t2_wh: number | null;
  export_t2_wh: number | null;
  gas_dm3: number | null;
}
