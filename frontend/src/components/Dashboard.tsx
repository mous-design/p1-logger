import { useEffect, useState } from "react";
import { DashboardHeader } from "./DashboardHeader";
import { ElectricityWidget } from "./ElectricityWidget";
import { GasWidget } from "./GasWidget";
import { OverviewWidget } from "./OverviewWidget";
import { PowerWidget } from "./PowerWidget";
import type { Metric, Widget, WidgetId } from "../types";

const WIDGETS: Widget[] = [
  { id: "overview", label: "Tellerstanden" },
  { id: "power", label: "Vermogen" },
  { id: "electricity", label: "Elektraverbruik" },
  { id: "gas", label: "Gasverbruik" },
];

const DEFAULT_ENABLED: Record<WidgetId, boolean> = { overview: true, power: true, electricity: true, gas: true };

// localStorage, not a backend setting -- which widgets are on/off is a
// per-browser display preference, not something that needs to sync across
// devices or survive a p1-web restart on the server side.
const STORAGE_KEY = "p1-web:enabled-widgets";

function loadEnabledWidgets(): Record<WidgetId, boolean> {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    return raw ? { ...DEFAULT_ENABLED, ...JSON.parse(raw) } : DEFAULT_ENABLED;
  } catch {
    return DEFAULT_ENABLED;
  }
}

interface DashboardProps {
  onOpenDetail: (metric: Metric) => void;
}

export function Dashboard({ onOpenDetail }: DashboardProps) {
  const [enabled, setEnabled] = useState<Record<WidgetId, boolean>>(loadEnabledWidgets);

  useEffect(() => {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(enabled));
  }, [enabled]);

  function toggle(id: WidgetId) {
    setEnabled((prev) => ({ ...prev, [id]: !prev[id] }));
  }

  return (
    <div>
      <DashboardHeader widgets={WIDGETS} enabled={enabled} onToggle={toggle} />
      <div className="dashboard-grid">
        {enabled.overview && <OverviewWidget />}
        {enabled.power && <PowerWidget onOpen={() => onOpenDetail("power")} />}
        {enabled.electricity && <ElectricityWidget onOpen={() => onOpenDetail("electricity")} />}
        {enabled.gas && <GasWidget onOpen={() => onOpenDetail("gas")} />}
      </div>
    </div>
  );
}
