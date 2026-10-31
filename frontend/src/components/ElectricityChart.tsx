import { useMemo, useState } from "react";
import { CartesianGrid, Line, LineChart, ReferenceLine, ResponsiveContainer, Tooltip, XAxis, YAxis } from "recharts";
import { electricityDeltas, electricityTotals } from "../chart/deltas";
import { computeTicks, computeValueTicks, formatTick, formatTooltipLabel } from "../chart/ticks";
import { useAxisZoom } from "../chart/useAxisZoom";
import { AXIS_TICK_FONT_SIZE, CHART_MARGIN, X_AXIS_HEIGHT, Y_AXIS_WIDTH } from "../chart/zoom";
import { AxisScrollbar } from "../chart/AxisScrollbar";
import { computeYDomain, type YDomainOptions } from "../chart/range";
import { ZoomIconGutter } from "../chart/ZoomIconGutter";
import type { ElectricityPoint, Period } from "../types";
import { formatEuro, formatMilli, tooltipFormatter } from "../chart/format";
import { electricityCosts } from "../chart/costs";
import { useTariffs } from "../hooks/useTariffs";

// Shared with ElectricityWidget's compact MiniChart, so the tile and the
// detail view always use the same colors for T1/T2.
export const ELECTRICITY_SERIES = [
  { dataKey: "net_t1_wh", name: "T1", color: "#8884d8" },
  { dataKey: "net_t2_wh", name: "T2", color: "#ff7300" },
] as const;

// "Totaal", not "Gecombineerd", to match the wording already used in the
// summary line below ("Totaal X kWh") -- same word, one less thing to
// reconcile. Exported separately (not just inlined into
// ELECTRICITY_DETAIL_SERIES) so ElectricityWidget's tile can show just this
// one line, matching what the detail view now defaults to.
export const ELECTRICITY_COMBINED_SERIES = [{ dataKey: "combined_wh", name: "Totaal", color: "#82ca9d" }] as const;

const ELECTRICITY_DETAIL_SERIES = [...ELECTRICITY_SERIES, ...ELECTRICITY_COMBINED_SERIES] as const;
const ELECTRICITY_KEYS = ELECTRICITY_DETAIL_SERIES.map((s) => s.dataKey);

// Shared with ElectricityWidget, so tile and detail view scale the same way.
// Net usage per hour is signed (export below 0), so 0 is the line that
// matters; flatSpan (in Wh) keeps 1-decimal kWh tick labels distinct.
export const ELECTRICITY_Y_DOMAIN: YDomainOptions = { includeZero: true, flatSpan: 1000 };

// Only the combined total on by default -- T1/T2 individually mostly
// matters when you're specifically comparing tariffs, not for the everyday
// "how much did I use" glance.
const HIDDEN_BY_DEFAULT = new Set(["net_t1_wh", "net_t2_wh"]);

// One decimal, not two: the same minus-sign-clipping issue the power chart
// hit applies here too (Y_AXIS_WIDTH is shared, and "-1.62 kWh" doesn't fit
// where "-1.6 kWh" does) -- one decimal is still plenty of resolution for
// an axis tick at this scale.
export function formatKwh(wh: number): string {
  return formatMilli(wh, "kWh", 1);
}

const formatTooltipValueKwh = tooltipFormatter((wh) => formatMilli(wh, "kWh", 3));

// A standalone headline number gets more precision than an axis tick does --
// no shared-width constraint to worry about here. "—" for a register total
// that can't be computed yet (see ElectricityTotals).
function formatKwhSummary(wh: number | null): string {
  return wh === null ? "—" : formatMilli(wh, "kWh", 2);
}

// kWh with its price after it: `eur` undefined means no tariffs are
// configured (no price shown at all), null means not computable ("—").
function formatKwhWithEuro(wh: number | null, eur: number | null | undefined): string {
  if (eur === undefined) return formatKwhSummary(wh);
  return `${formatKwhSummary(wh)} (${eur === null ? "—" : formatEuro(eur)})`;
}

function minus(a: number | null, b: number | null): number | null {
  return a === null || b === null ? null : a - b;
}

function plus(a: number | null, b: number | null): number | null {
  return a === null || b === null ? null : a + b;
}

interface ElectricityChartProps {
  period: Period;
  start: number;
  end: number;
  baseline: ElectricityPoint | undefined;
  points: ElectricityPoint[];
}

export function ElectricityChart({ period, start, end, baseline, points }: ElectricityChartProps) {
  const deltaPoints = useMemo(() => electricityDeltas(points, baseline), [points, baseline]);
  const totals = useMemo(() => electricityTotals(points, baseline), [points, baseline]);
  const tariffs = useTariffs();
  const costs = useMemo(() => (tariffs ? electricityCosts(points, baseline, tariffs) : null), [points, baseline, tariffs]);
  // A tariff's own total is what it cost net: used minus delivered back.
  // `netEur` is null without tariffs, so `netEur?.t1` reads as undefined
  // ("no price shown") -- distinct from a null t1 ("not computable").
  const netEur = costs && { t1: minus(costs.importT1Eur, costs.exportT1Eur), t2: minus(costs.importT2Eur, costs.exportT2Eur) };
  const domainY = useMemo(() => computeYDomain(deltaPoints, ELECTRICITY_KEYS, ELECTRICITY_Y_DOMAIN), [deltaPoints]);
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
      {/* Per tariff, delivered and used separately -- what's needed to price
          a period, since delivering and using are paid at different rates. */}
      <p className="register-totals">
        T1: geleverd {formatKwhWithEuro(totals.exportT1Wh, costs?.exportT1Eur)}, verbruikt{" "}
        {formatKwhWithEuro(totals.importT1Wh, costs?.importT1Eur)}, totaal {formatKwhWithEuro(totals.netT1Wh, netEur?.t1)}
      </p>
      <p className="register-totals">
        T2: geleverd {formatKwhWithEuro(totals.exportT2Wh, costs?.exportT2Eur)}, verbruikt{" "}
        {formatKwhWithEuro(totals.importT2Wh, costs?.importT2Eur)}, totaal {formatKwhWithEuro(totals.netT2Wh, netEur?.t2)}
      </p>
      <p className="net-total">
        Totaal: {formatKwhWithEuro(totals.netT1Wh + totals.netT2Wh, netEur ? plus(netEur.t1, netEur.t2) : undefined)}
      </p>
      <div className="series-toggle-row">
        {ELECTRICITY_DETAIL_SERIES.map((s) => {
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
                  ticks={valueTicks}
                  allowDataOverflow
                  tickFormatter={formatKwh}
                  tick={{ fontSize: AXIS_TICK_FONT_SIZE }}
                />
                <ReferenceLine y={0} className="zero-line" />
                <Tooltip labelFormatter={(t) => formatTooltipLabel(period, t)} formatter={formatTooltipValueKwh} />
                {ELECTRICITY_DETAIL_SERIES.map((s) => (
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
