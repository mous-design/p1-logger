import { useEffect, useState } from "react";
import { ElectricityChart } from "./ElectricityChart";
import { GasChart } from "./GasChart";
import { PeriodSelector } from "./PeriodSelector";
import { PowerChart } from "./PowerChart";
import { formatHeader } from "../chart/ticks";
import { useSeries } from "../hooks/useSeries";
import { detailPath, parsePeriodAnchor } from "../url";
import type { Metric, Period } from "../types";

interface DetailViewProps {
  initialMetric: Metric;
  onBack: () => void;
}

/** The full single-chart view -- period selector, zoom, everything that a
 * compact widget tile deliberately leaves out. `initialMetric` picks which
 * chart to render for this view's whole lifetime -- switching metrics now
 * happens by going back to the dashboard and opening a different widget, so
 * unlike period/anchor there's no in-place metric switcher here anymore. */
export function DetailView({ initialMetric, onBack }: DetailViewProps) {
  // Falls back to day/now only for whichever half is missing from the URL
  // (a bare /detail/power, or a query string someone hand-edited down to
  // just `?period=`) -- not an all-or-nothing parse.
  const urlState = parsePeriodAnchor(window.location.search);
  const [period, setPeriod] = useState<Period>(() => urlState.period ?? "day");
  const [anchor, setAnchor] = useState(() => urlState.anchor ?? Math.floor(Date.now() / 1000));
  const { data, loading, error } = useSeries(initialMetric, period, anchor);

  // Keeps the URL a true reflection of what's on screen, not just the
  // metric -- replaceState (not pushState) so clicking through periods
  // doesn't flood browser history with an entry per click.
  useEffect(() => {
    window.history.replaceState(null, "", `${detailPath(initialMetric)}?period=${period}&anchor=${anchor}`);
  }, [initialMetric, period, anchor]);

  return (
    <div className="detail-view">
      <button type="button" className="back-button" onClick={onBack}>
        ← Dashboard
      </button>
      <div className="detail-tile">
        <div className="detail-tile-header">
          <h2 className="detail-title">{data ? formatHeader(data.metric, data.period, data.start) : ""}</h2>
          <div className="detail-tile-header-controls">
            <PeriodSelector
              period={period}
              anchor={anchor}
              onSelect={(nextPeriod, nextAnchor) => {
                setPeriod(nextPeriod);
                setAnchor(nextAnchor);
              }}
            />
          </div>
        </div>
        <div className="detail-tile-body">
          {loading && <p>Laden…</p>}
          {error && <p>Fout: {error}</p>}
          {data?.metric === "power" && <PowerChart period={data.period} start={data.start} end={data.end} points={data.points} />}
          {data?.metric === "electricity" && (
            <ElectricityChart period={data.period} start={data.start} end={data.end} baseline={data.baseline} points={data.points} />
          )}
          {data?.metric === "gas" && <GasChart period={data.period} start={data.start} end={data.end} baseline={data.baseline} points={data.points} />}
        </div>
      </div>
    </div>
  );
}
