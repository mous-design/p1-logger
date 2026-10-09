import { useMemo } from "react";
import { electricityDeltas } from "../chart/deltas";
import { MiniChart } from "../chart/MiniChart";
import { useTodaySeries } from "../hooks/useTodaySeries";
import { ELECTRICITY_COMBINED_SERIES, formatKwh } from "./ElectricityChart";
import { WidgetTile } from "./WidgetTile";

export function ElectricityWidget({ onOpen }: { onOpen: () => void }) {
  const { data, loading, error } = useTodaySeries("electricity");
  const deltaPoints = useMemo(() => (data?.metric === "electricity" ? electricityDeltas(data.points, data.baseline) : []), [data]);

  return (
    <WidgetTile title="Elektraverbruik" onOpen={onOpen} loading={loading} error={error}>
      {data?.metric === "electricity" && (
        <MiniChart points={deltaPoints} series={ELECTRICITY_COMBINED_SERIES} height={280} formatY={formatKwh} />
      )}
    </WidgetTile>
  );
}
