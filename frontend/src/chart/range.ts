/** A plain numeric window -- used for both the x-axis (epoch seconds) and
 * the y-axis (watts/kWh/m³/...). Time and value are structurally the same
 * thing here (a continuous [start, end] range within a larger domain), so
 * one generic module drives both axes' zoom/pan instead of duplicating the
 * math per axis. */
export interface NumRange {
  start: number;
  end: number;
}

export function sameRange(a: NumRange, b: NumRange): boolean {
  return a.start === b.start && a.end === b.end;
}

export function rangeSpan(r: NumRange): number {
  return r.end - r.start;
}

function clampToDomain(range: NumRange, domain: NumRange): NumRange {
  const domainSpan = rangeSpan(domain);
  let span = Math.min(rangeSpan(range), domainSpan);
  let start = Math.max(domain.start, Math.min(range.start, domain.end - span));
  let end = start + span;
  // Floating point can push `end` a hair past `domain.end` -- pull both
  // back together rather than let allowDataOverflow clip a Line render.
  if (end > domain.end) {
    end = domain.end;
    start = end - span;
  }
  return { start, end };
}

// A span can shrink to at most this fraction of the full domain -- without
// a floor, zooming in repeatedly would approach a zero-width range and
// divide-by-zero the pixel<->value conversion used for dragging.
const MIN_ZOOM_FRACTION = 0.01;

export function zoomRange(range: NumRange, domain: NumRange, factor: number): NumRange {
  const center = (range.start + range.end) / 2;
  const minSpan = rangeSpan(domain) * MIN_ZOOM_FRACTION;
  const span = Math.max(minSpan, rangeSpan(range) * factor);
  return clampToDomain({ start: center - span / 2, end: center + span / 2 }, domain);
}

/** Shifts `range` by its own span in `direction`, clamped to `domain` --
 * the "click the scrollbar track" page-by-one-screen behavior of a
 * classic OS scrollbar, not just a plain drag-to-pan. */
export function pageRange(range: NumRange, domain: NumRange, direction: 1 | -1): NumRange {
  const span = rangeSpan(range);
  return clampToDomain({ start: range.start + direction * span, end: range.end + direction * span }, domain);
}

/** Shifts `range` by an arbitrary pixel-derived delta (dragging the thumb),
 * clamped to `domain`. */
export function panRange(range: NumRange, domain: NumRange, delta: number): NumRange {
  return clampToDomain({ start: range.start + delta, end: range.end + delta }, domain);
}

/** Resizes just one edge of `range` (dragging a handle), clamped so it
 * never crosses the other edge or the domain bounds. */
export function resizeRangeEdge(range: NumRange, domain: NumRange, edge: "start" | "end", value: number): NumRange {
  const minSpan = rangeSpan(domain) * MIN_ZOOM_FRACTION;
  if (edge === "start") {
    const start = Math.max(domain.start, Math.min(value, range.end - minSpan));
    return { start, end: range.end };
  }
  const end = Math.min(domain.end, Math.max(value, range.start + minSpan));
  return { start: range.start, end };
}

// Headroom above/below the raw min/max so a line doesn't touch the very
// top/bottom edge of the plot -- Recharts' own domain={['auto','auto']}
// added similar breathing room automatically; computing the domain by hand
// (so it can stay fixed across zoom/pan, see useAxisZoom) means doing that
// padding by hand too.
const Y_DOMAIN_PADDING_FRACTION = 0.08;

/** The full y-domain for a chart, computed once from *all* of a period's
 * data across the given series keys -- not just whatever's currently
 * visible, and not recomputed per zoom/toggle, so panning/zooming and
 * hiding a series never rescales the axis out from under the reader.
 *
 * `includeZero` is for signed metrics where 0 is the line that matters
 * (power: import above, export below) -- without it, a consumption-only
 * night would scale to e.g. 0.2-0.4 kW and hide the baseline entirely. No
 * padding is added past a zero edge: that would only add an empty strip
 * beyond the baseline (a "-0.2 kW" tick on a night with no export at all). */
export function computeYDomain<T>(
  points: T[],
  keys: readonly (keyof T)[],
  { includeZero = false }: { includeZero?: boolean } = {},
): NumRange {
  let min = Infinity;
  let max = -Infinity;
  for (const p of points) {
    for (const key of keys) {
      const value = p[key];
      if (typeof value !== "number") continue;
      if (value < min) min = value;
      if (value > max) max = value;
    }
  }
  if (!Number.isFinite(min) || !Number.isFinite(max)) {
    return { start: 0, end: 1 };
  }
  if (includeZero) {
    min = Math.min(min, 0);
    max = Math.max(max, 0);
  }
  if (min === max) {
    return { start: min - 1, end: max + 1 };
  }
  const padding = (max - min) * Y_DOMAIN_PADDING_FRACTION;
  return {
    start: includeZero && min === 0 ? 0 : min - padding,
    end: includeZero && max === 0 ? 0 : max + padding,
  };
}
