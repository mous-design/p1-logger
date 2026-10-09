import { formatMilli } from "../chart/format";
import { useCurrentReadings } from "../hooks/useCurrentReadings";
import { GAS_COLOR } from "./GasChart";

const POLL_INTERVAL_MS = 5000;

// Design tokens (App.scss) -- single source of truth, so the import/export
// palette can be retuned in one place without hunting down every consumer.
const IMPORT_COLOR = "var(--import)";
const EXPORT_COLOR = "var(--export)";

// Live power gets more precision than the charts' axis ticks -- a reading
// that refreshes every few seconds is where a 10 W change should be
// visible. The meter registers are large running totals, so one decimal
// already reads like the physical meter's own display.
const formatPower = (w: number) => formatMilli(w, "kW", 2);
const formatRegister = (milli: number, unit: "kWh" | "m³") => formatMilli(milli, unit, 1);

function Stat({ label, value, color }: { label: string; value: string; color?: string }) {
  return (
    <div className="overview-stat">
      <dt>{label}</dt>
      <dd style={color ? { color } : undefined}>{value}</dd>
    </div>
  );
}

/** Live numbers, not a chart -- polls `/api/current` (today's raw day-file,
 * see queries.rs) every few seconds. The literal tellerstanden as they'd
 * read on the physical meter (T1/T2 import and export, unrelated to each
 * other -- no netting here, unlike the Elektra chart's delta view), plus
 * one netted number for the instantaneous power since a live wattage is
 * naturally a single directional value. Not click-to-detail like the other
 * tiles: these are already the plainest possible view of the data. */
export function OverviewWidget() {
  const { data, error } = useCurrentReadings(POLL_INTERVAL_MS);

  return (
    <div className="widget-tile overview-widget">
      <div className="widget-tile-title">Tellerstanden</div>
      {error && <p className="widget-tile-status">Fout: {error}</p>}
      {!error && !data && <p className="widget-tile-status">Laden…</p>}
      {data && (
        <dl className="overview-stats">
          <Stat
            label="Vermogen (netto)"
            value={data.power_w != null ? formatPower(data.power_w) : "—"}
            color={data.power_w == null || data.power_w === 0 ? undefined : data.power_w > 0 ? IMPORT_COLOR : EXPORT_COLOR}
          />
          <Stat label="T1 import" value={data.import_t1_wh != null ? formatRegister(data.import_t1_wh, "kWh") : "—"} color={IMPORT_COLOR} />
          <Stat label="T2 import" value={data.import_t2_wh != null ? formatRegister(data.import_t2_wh, "kWh") : "—"} color={IMPORT_COLOR} />
          <Stat label="T1 export" value={data.export_t1_wh != null ? formatRegister(data.export_t1_wh, "kWh") : "—"} color={EXPORT_COLOR} />
          <Stat label="T2 export" value={data.export_t2_wh != null ? formatRegister(data.export_t2_wh, "kWh") : "—"} color={EXPORT_COLOR} />
          <Stat label="Gas" value={data.gas_dm3 != null ? formatRegister(data.gas_dm3, "m³") : "—"} color={GAS_COLOR} />
        </dl>
      )}
    </div>
  );
}
