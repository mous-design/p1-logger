import { useMemo } from "react";
import { CartesianGrid, Line, LineChart, ReferenceLine, ResponsiveContainer, XAxis, YAxis } from "recharts";
import { computeYDomain } from "./range";
import { computeValueTicks } from "./ticks";

export interface MiniChartSeries {
  dataKey: string;
  color: string;
}

interface MiniChartProps<T> {
  points: T[];
  series: readonly MiniChartSeries[];
  height?: number;
  /** Formats the y-axis' handful of ticks in the metric's own unit (kW,
   * kWh, m³, ...) -- MiniChart itself doesn't know which metric it's
   * showing, so each widget supplies its own detail-view formatter. */
  formatY: (value: number) => string;
  /** Always keep 0 in view and mark it with a solid line -- for signed
   * metrics like power, where 0 is the import/export boundary. */
  zeroLine?: boolean;
}

const MINI_TICK_FONT_SIZE = 9;

function formatTimeTick(t: number): string {
  return new Date(t * 1000).toLocaleTimeString("nl-NL", { hour: "2-digit", minute: "2-digit" });
}

/** Small trend line for a dashboard widget tile -- a light axis with a
 * handful of ticks for orientation, but none of the detail view's zoom,
 * legend, or tooltip. Always "today" in practice (every widget fixes its
 * own period), so the x-axis ticks stay time-only. */
export function MiniChart<T extends { t: number }>({ points, series, height = 260, formatY, zeroLine = false }: MiniChartProps<T>) {
  // Same domain + tick helpers as the detail view, so 0 is both in view and
  // a tick -- Recharts' own auto-domain can't be told to include 0 while
  // still rounding its ticks to nice values (a function domain gets
  // evenly-spaced ticks across the raw bounds instead, e.g. -2.7/1.3/4.6).
  const zeroDomain = useMemo(
    () => (zeroLine ? computeYDomain(points, series.map((s) => s.dataKey as keyof T), { includeZero: true }) : null),
    [zeroLine, points, series],
  );
  return (
    <ResponsiveContainer width="100%" height={height}>
      <LineChart data={points} margin={{ top: 6, right: 8, bottom: 0, left: 0 }}>
        <CartesianGrid strokeDasharray="3 3" />
        <XAxis
          dataKey="t"
          type="number"
          domain={["dataMin", "dataMax"]}
          tickFormatter={formatTimeTick}
          tick={{ fontSize: MINI_TICK_FONT_SIZE }}
          tickCount={3}
        />
        <YAxis
          domain={zeroDomain ? [zeroDomain.start, zeroDomain.end] : ["auto", "auto"]}
          ticks={zeroDomain ? computeValueTicks(zeroDomain.start, zeroDomain.end, 5) : undefined}
          tickFormatter={formatY}
          tick={{ fontSize: MINI_TICK_FONT_SIZE }}
          tickCount={3}
          width={60}
        />
        {zeroLine && <ReferenceLine y={0} className="zero-line" />}
        {series.map((s) => (
          <Line
            key={s.dataKey}
            type="monotone"
            dataKey={s.dataKey}
            stroke={s.color}
            strokeWidth={1.5}
            dot={false}
            isAnimationActive={false}
          />
        ))}
      </LineChart>
    </ResponsiveContainer>
  );
}
