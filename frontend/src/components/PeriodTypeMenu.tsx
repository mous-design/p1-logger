import { usePopupMenu } from "../hooks/usePopupMenu";
import type { Period } from "../types";

const PERIODS: { period: Period; label: string }[] = [
  { period: "day", label: "Dag" },
  { period: "week", label: "Week" },
  { period: "month", label: "Maand" },
  { period: "year", label: "Jaar" },
  { period: "5year", label: "5 jaar" },
];

interface PeriodTypeMenuProps {
  period: Period;
  onSelect: (period: Period) => void;
}

/** Popup period-type picker, same interaction/visual pattern as the
 * dashboard's cog menu (DashboardHeader.tsx) -- click to open, click
 * outside to close, .widget-toggle-menu's own popup styling reused as-is.
 * The trigger itself deliberately stays a plain, unstyled <button> (no
 * className) so it looks exactly like the "Vandaag" button next to it,
 * not like an icon button. */
export function PeriodTypeMenu({ period, onSelect }: PeriodTypeMenuProps) {
  const { open, setOpen, menuRef } = usePopupMenu();

  const currentLabel = PERIODS.find((p) => p.period === period)?.label ?? "";

  return (
    <div className="period-type-menu" ref={menuRef}>
      <button type="button" onClick={() => setOpen((o) => !o)}>
        {currentLabel}
      </button>
      {open && (
        <div className="widget-toggle-menu">
          {PERIODS.map((p) => (
            <button
              key={p.period}
              type="button"
              disabled={p.period === period}
              onClick={() => {
                onSelect(p.period);
                setOpen(false);
              }}
            >
              {p.label}
            </button>
          ))}
        </div>
      )}
    </div>
  );
}
