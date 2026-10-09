import { useMemo } from "react";
import { CartesianGrid, Line, LineChart, ResponsiveContainer, Tooltip, XAxis, YAxis } from "recharts";
import { gasDeltas, gasNetTotal } from "../chart/deltas";
import { computeTicks, formatTick, formatTooltipLabel } from "../chart/ticks";
import { useAxisZoom } from "../chart/useAxisZoom";
import { AXIS_TICK_FONT_SIZE, CHART_MARGIN, X_AXIS_HEIGHT, Y_AXIS_WIDTH } from "../chart/zoom";
import { AxisScrollbar } from "../chart/AxisScrollbar";
import { computeYDomain } from "../chart/range";
import { ZoomIconGutter } from "../chart/ZoomIconGutter";
import type { GasPoint, Period } from "../types";
import { formatMilli, tooltipFormatter } from "../chart/format";

// Shared with GasWidget's compact MiniChart and the Tellerstanden tile, so
// gas always reads in the same color. Not a CSS token like --import/--export:
// it's passed as a Recharts stroke, an SVG attribute where var() isn't
// reliably resolved.
export const GAS_COLOR = "#8884d8";
export const GAS_SERIES = [{ dataKey: "import_dm3", name: "verbruik", color: GAS_COLOR }] as const;
const GAS_KEYS = GAS_SERIES.map((s) => s.dataKey);

// dm3 -> m3: matches the unit gasmeters and bills are normally read in.
export function formatM3(dm3: number): string {
  return formatMilli(dm3, "m³", 2);
}

const formatTooltipValueM3 = tooltipFormatter((dm3) => formatMilli(dm3, "m³", 3));

// A standalone headline number gets more precision than an axis tick does --
// no shared-width constraint to worry about here.
function formatM3Summary(dm3: number): string {
  return formatMilli(dm3, "m³", 2);
}

interface GasChartProps {
  period: Period;
  start: number;
  end: number;
  baseline: GasPoint | undefined;
  points: GasPoint[];
}

export function GasChart({ period, start, end, baseline, points }: GasChartProps) {
  const deltaPoints = useMemo(() => gasDeltas(points, baseline), [points, baseline]);
  const total = useMemo(() => gasNetTotal(points, baseline), [points, baseline]);
  const domainY = useMemo(() => computeYDomain(deltaPoints, GAS_KEYS), [deltaPoints]);
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

  return (
    <div>
      <p className="net-total">Verbruikt: {formatM3Summary(total)}</p>
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
              <LineChart data={deltaPoints} margin={CHART_MARGIN}>
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
                  allowDataOverflow
                  tickFormatter={formatM3}
                  tick={{ fontSize: AXIS_TICK_FONT_SIZE }}
                />
                <Tooltip labelFormatter={(t) => formatTooltipLabel(period, t)} formatter={formatTooltipValueM3} />
                {GAS_SERIES.map((s) => (
                  <Line key={s.dataKey} type="monotone" dataKey={s.dataKey} stroke={s.color} dot={false} name={s.name} isAnimationActive={false} />
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
