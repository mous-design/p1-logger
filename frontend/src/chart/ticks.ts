import type { ReactNode } from "react";
import type { Metric, Period } from "../types";

const METRIC_LABEL: Record<Metric, string> = {
  power: "Vermogen",
  electricity: "Elektraverbruik",
  gas: "Gasverbruik",
};

/** "op woensdag 22 juli 2026" / "in de week van 20 juli 2026" / etc --
 * the part of the header that names the period, independent of which
 * metric it's attached to. */
function formatPeriodPhrase(period: Period, start: number): string {
  const date = new Date(start * 1000);
  switch (period) {
    case "day":
      return `op ${date.toLocaleDateString("nl-NL", { weekday: "long", year: "numeric", month: "long", day: "numeric" })}`;
    case "week":
      return `in de week van ${date.toLocaleDateString("nl-NL", { day: "numeric", month: "long", year: "numeric" })}`;
    case "month":
      return `in de maand ${date.toLocaleDateString("nl-NL", { month: "long", year: "numeric" })}`;
    case "year":
      return `in het jaar ${date.toLocaleDateString("nl-NL", { year: "numeric" })}`;
    case "5year": {
      // `start` is the first of the 5 calendar years (see period.rs's
      // range()), so the range's own last year is 4 years later.
      const firstYear = date.getFullYear();
      return `in de jaren ${firstYear}-${firstYear + 4}`;
    }
  }
}

/** Full header -- shown once above the chart, e.g. "Vermogen op woensdag 22
 * juli 2026" or "Gasverbruik in de maand juli 2026". Reads as one sentence
 * combining what's being shown (the metric) with which period it covers. */
export function formatHeader(metric: Metric, period: Period, start: number): string {
  return `${METRIC_LABEL[metric]} ${formatPeriodPhrase(period, start)}`;
}

/** Axis ticks stay short: for day, the header already carries the date, so
 * the tick only needs the time-of-day. Week still needs a weekday on each
 * tick despite the header also showing a date -- a week has several
 * distinct days in it, and ticks naturally land once a day (see
 * computeTicks below), so a time-only label like "day" uses would repeat
 * the same "02:00" on every single tick with nothing to tell them apart.
 * Month/year drop the year -- the header already carries it, and every
 * tick within a single month/year view shares that same year anyway.
 * 5year is the one range that spans several different years, so that's
 * the one case where the tick itself still needs to show it. */
export function formatTick(period: Period, t: number): string {
  const date = new Date(t * 1000);
  switch (period) {
    case "day":
      return date.toLocaleTimeString("nl-NL", { hour: "2-digit", minute: "2-digit" });
    case "week":
      return date.toLocaleDateString("nl-NL", { weekday: "short", day: "numeric", month: "numeric" });
    case "month":
    case "year":
      return date.toLocaleDateString("nl-NL", { day: "numeric", month: "numeric" });
    case "5year":
      return date.toLocaleDateString("nl-NL");
  }
}

const SECONDS_PER_HOUR = 3600;
const SECONDS_PER_DAY = 24 * SECONDS_PER_HOUR;

const NICE_STEPS_SECONDS = [
  60,
  5 * 60,
  10 * 60,
  15 * 60,
  30 * 60,
  SECONDS_PER_HOUR,
  2 * SECONDS_PER_HOUR,
  3 * SECONDS_PER_HOUR,
  6 * SECONDS_PER_HOUR,
  12 * SECONDS_PER_HOUR,
  SECONDS_PER_DAY,
  2 * SECONDS_PER_DAY,
  3 * SECONDS_PER_DAY,
  5 * SECONDS_PER_DAY,
  7 * SECONDS_PER_DAY,
  14 * SECONDS_PER_DAY,
  30 * SECONDS_PER_DAY,
  60 * SECONDS_PER_DAY,
  90 * SECONDS_PER_DAY,
  180 * SECONDS_PER_DAY,
  365 * SECONDS_PER_DAY,
  2 * 365 * SECONDS_PER_DAY,
];

/** Picks the smallest "nice" step (1/5/10/15/30 min, 1/2/3/6/12/24h) that
 * gives roughly a dozen ticks across whatever range is currently visible
 * -- the full period by default, or a much narrower slice once zoomed via
 * the Brush. A fixed step wouldn't work for both: 2-hour ticks are right
 * for a full day but way too sparse once you've zoomed into 20 minutes. */
export function computeTicks(visibleStart: number, visibleEnd: number): number[] {
  const target = (visibleEnd - visibleStart) / 12;
  const step = NICE_STEPS_SECONDS.find((s) => s >= target) ?? NICE_STEPS_SECONDS[NICE_STEPS_SECONDS.length - 1];
  const firstTick = Math.ceil(visibleStart / step) * step;
  const ticks: number[] = [];
  for (let t = firstTick; t <= visibleEnd; t += step) {
    ticks.push(t);
  }
  return ticks;
}

/** "Nice" value-axis ticks (1/2/5 x 10^n) for whatever y-range is currently
 * visible, always on whole multiples of the step -- so 0 is a tick whenever
 * it's in view. Recharts' own ticks for a fixed domain start counting from
 * the domain's raw (padded) minimum instead, landing on arbitrary values
 * like -3.2 / -1.9 / -0.6 kW that skip 0 altogether. */
export function computeValueTicks(visibleStart: number, visibleEnd: number, targetCount: number): number[] {
  const target = (visibleEnd - visibleStart) / targetCount;
  if (!(target > 0)) return [];
  const magnitude = 10 ** Math.floor(Math.log10(target));
  const step = [1, 2, 5].map((m) => m * magnitude).find((s) => s >= target) ?? 10 * magnitude;
  const ticks: number[] = [];
  for (let i = Math.ceil(visibleStart / step); i * step <= visibleEnd; i++) {
    ticks.push(i * step);
  }
  return ticks;
}

// The tooltip label reuses the same time-only format as the axis ticks --
// the header above the chart already carries the date, so repeating it on
// every hover would just be clutter. Recharts' labelFormatter type is
// looser than tickFormatter's (it can receive any ReactNode), even though
// `t` here is always our own numeric data.
export function formatTooltipLabel(period: Period, t: ReactNode): string {
  return typeof t === "number" ? formatTick(period, t) : String(t);
}
