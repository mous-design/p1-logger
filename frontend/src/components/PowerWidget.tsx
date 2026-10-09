import { MiniChart } from "../chart/MiniChart";
import { useTodaySeries } from "../hooks/useTodaySeries";
import { formatKw, POWER_SERIES } from "./PowerChart";
import { WidgetTile } from "./WidgetTile";

// Just avg, matching the detail view's own default-visible series -- a
// glance at the tile shouldn't show more lines than the "normal" view does.
const WIDGET_SERIES = POWER_SERIES.filter((s) => s.dataKey === "avg_w");

export function PowerWidget({ onOpen }: { onOpen: () => void }) {
  const { data, loading, error } = useTodaySeries("power");

  return (
    <WidgetTile title="Vermogen" onOpen={onOpen} loading={loading} error={error}>
      {data?.metric === "power" && <MiniChart points={data.points} series={WIDGET_SERIES} height={280} formatY={formatKw} zeroLine />}
    </WidgetTile>
  );
}
