import { useMemo, useState } from "react";
import { CartesianGrid, Line, LineChart, ReferenceLine, ResponsiveContainer, Tooltip, XAxis, YAxis } from "recharts";
import { computeTicks, computeValueTicks, formatTick, formatTooltipLabel } from "../chart/ticks";
import { useAxisZoom } from "../chart/useAxisZoom";
import { AXIS_TICK_FONT_SIZE, CHART_MARGIN, X_AXIS_HEIGHT, Y_AXIS_WIDTH } from "../chart/zoom";
import { AxisScrollbar } from "../chart/AxisScrollbar";
import { computeYDomain } from "../chart/range";
import { ZoomIconGutter } from "../chart/ZoomIconGutter";
import type { Period, PowerPoint } from "../types";
import { formatMilli, tooltipFormatter } from "../chart/format";

// Tooltip lists items in Line-render order (min, avg, max), but that's the
// wrong order to read on hover -- max/avg/min matches how the eye scans the
// chart top to bottom. itemSorter only reorders the tooltip's own list, not
// the SVG paint order the Lines are declared in below.
const TOOLTIP_ORDER = ["max_w", "avg_w", "min_w"];
function tooltipItemSorter(item: { dataKey?: unknown }): number {
  const index = TOOLTIP_ORDER.indexOf(String(item.dataKey));
  return index === -1 ? TOOLTIP_ORDER.length : index;
}

// kW, not W: household power here is a handful of kW, so "-3000 W" is both
// harder to read at a glance and long enough to get clipped against the
// chart's left edge (the minus sign was falling outside the axis's
// auto-computed width) -- "-3.0 kW" is shorter and reads better.
export function formatKw(watts: number): string {
  return formatMilli(watts, "kW", 1);
}

const formatTooltipValueKw = tooltipFormatter((watts) => formatMilli(watts, "kW", 2));

// Shared with PowerWidget's compact MiniChart, so the tile and the detail
// view always use the same colors for min/avg/max.
export const POWER_SERIES = [
  { dataKey: "min_w", name: "min", color: "#8884d8" },
  { dataKey: "avg_w", name: "avg", color: "#82ca9d" },
  { dataKey: "max_w", name: "max", color: "#ff7300" },
] as const;

const POWER_KEYS = POWER_SERIES.map((s) => s.dataKey);

// Only avg on by default -- min/max mostly add clutter (a brief dip/spike)
// unless you're specifically looking for the extremes; avg alone already
// shows the day's overall shape.
const HIDDEN_BY_DEFAULT = new Set(["min_w", "max_w"]);

interface PowerChartProps {
  period: Period;
  start: number;
  end: number;
  points: PowerPoint[];
}

export function PowerChart({ period, start, end, points }: PowerChartProps) {
  // The y-domain is computed once from *all* of min/avg/max across the
  // whole period, independent of which series are currently toggled off --
  // otherwise hiding "max" would visibly rescale the axis, on top of the
  // rescale-on-zoom this domain is already meant to avoid.
  const domainY = useMemo(() => computeYDomain(points, POWER_KEYS, { includeZero: true }), [points]);
  const {
    rangeX,
    rangeY,
    domainX,
    setRangeX,
    setRangeY,
    pageX,
    pageY,
    zoomInX,
    zoomOutX,
    resetX,
    zoomInY,
    zoomOutY,
    resetY,
    canZoomInX,
    canZoomOutX,
    canResetX,
    canZoomInY,
    canZoomOutY,
    canResetY,
  } = useAxisZoom({ start, end }, domainY);
  const ticks = useMemo(() => computeTicks(rangeX.start, rangeX.end), [rangeX.start, rangeX.end]);
  const valueTicks = useMemo(() => computeValueTicks(rangeY.start, rangeY.end, 8), [rangeY.start, rangeY.end]);
  const [hiddenKeys, setHiddenKeys] = useState<Set<string>>(HIDDEN_BY_DEFAULT);

  function toggleSeries(dataKey: string) {
    setHiddenKeys((prev) => {
      const next = new Set(prev);
      if (next.has(dataKey)) {
        next.delete(dataKey);
      } else {
        next.add(dataKey);
      }
      return next;
    });
  }

  return (
    <div>
      <p className="sign-legend">Positief = verbruik (import), negatief = teruglevering (export)</p>
      {/* Recharts' own <Legend> doesn't preserve the order series are
          declared in (its collected payload ends up sorted active-before-
          inactive) -- a plain button row gives full control over both
          order and the dimmed "inactive" look. */}
      <div className="series-toggle-row">
        {POWER_SERIES.map((s) => {
          const active = !hiddenKeys.has(s.dataKey);
          return (
            <button
              key={s.dataKey}
              type="button"
              className="series-toggle"
              style={{ color: active ? s.color : "#bbb" }}
              onClick={() => toggleSeries(s.dataKey)}
            >
              <span className="series-toggle-dot" style={{ background: active ? s.color : "#ddd" }} />
              {s.name}
            </button>
          );
        })}
      </div>
      <div className="chart-area">
        <AxisScrollbar orientation="vertical" domain={domainY} range={rangeY} onChange={setRangeY} onPage={pageY} />
        <ZoomIconGutter
          className="zoom-icon-gutter-vertical"
          canZoomIn={canZoomInY}
          canZoomOut={canZoomOutY}
          canReset={canResetY}
          onZoomIn={zoomInY}
          onZoomOut={zoomOutY}
          onReset={resetY}
        />
        <div className="chart-area-plot">
            <ResponsiveContainer width="100%" height={360}>
              <LineChart data={points} margin={CHART_MARGIN}>
                <CartesianGrid strokeDasharray="3 3" />
                <XAxis
                  dataKey="t"
                  type="number"
                  height={X_AXIS_HEIGHT}
                  domain={[rangeX.start, rangeX.end]}
                  ticks={ticks}
                  tickFormatter={(t: number) => formatTick(period, t)}
                  allowDataOverflow
                  tick={{ fontSize: AXIS_TICK_FONT_SIZE }}
                />
                <YAxis
                  width={Y_AXIS_WIDTH}
                  domain={[rangeY.start, rangeY.end]}
                  ticks={valueTicks}
                  allowDataOverflow
                  tickFormatter={formatKw}
                  tick={{ fontSize: AXIS_TICK_FONT_SIZE }}
                />
                {/* 0 is the import/export boundary (see the sign legend above)
                    -- the one line that matters most, so it gets a solid
                    stroke instead of blending into the dashed grid. */}
                <ReferenceLine y={0} className="zero-line" />
                <Tooltip
                  labelFormatter={(t) => formatTooltipLabel(period, t)}
                  formatter={formatTooltipValueKw}
                  itemSorter={tooltipItemSorter}
                />
                {POWER_SERIES.map((s) => (
                  <Line
                    key={s.dataKey}
                    type="monotone"
                    dataKey={s.dataKey}
                    stroke={s.color}
                    dot={false}
                    name={s.name}
                    hide={hiddenKeys.has(s.dataKey)}
                    isAnimationActive={false}
                  />
                ))}
              </LineChart>
            </ResponsiveContainer>
          </div>
        <ZoomIconGutter
          className="zoom-icon-gutter-horizontal"
          canZoomIn={canZoomInX}
          canZoomOut={canZoomOutX}
          canReset={canResetX}
          onZoomIn={zoomInX}
          onZoomOut={zoomOutX}
          onReset={resetX}
        />
        <AxisScrollbar orientation="horizontal" domain={domainX} range={rangeX} onChange={setRangeX} onPage={pageX} />
      </div>
    </div>
  );
}
