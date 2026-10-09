import { useMemo } from "react";
import { gasDeltas } from "../chart/deltas";
import { MiniChart } from "../chart/MiniChart";
import { useTodaySeries } from "../hooks/useTodaySeries";
import { formatM3, GAS_SERIES } from "./GasChart";
import { WidgetTile } from "./WidgetTile";

export function GasWidget({ onOpen }: { onOpen: () => void }) {
  const { data, loading, error } = useTodaySeries("gas");
  const deltaPoints = useMemo(() => (data?.metric === "gas" ? gasDeltas(data.points, data.baseline) : []), [data]);

  return (
    <WidgetTile title="Gasverbruik" onOpen={onOpen} loading={loading} error={error}>
      {data?.metric === "gas" && <MiniChart points={deltaPoints} series={GAS_SERIES} height={280} formatY={formatM3} />}
    </WidgetTile>
  );
}
