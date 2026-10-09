import { PeriodTypeMenu } from "./PeriodTypeMenu";
import type { Period } from "../types";

// Label for the "jump to now" button, shaped per period so it reads right
// regardless of which one is selected ("Vandaag" would be wrong for "Jaar").
const CURRENT_LABEL: Record<Period, string> = {
  day: "Vandaag",
  week: "Deze week",
  month: "Deze maand",
  year: "Dit jaar",
  "5year": "Laatste 5 jaar",
};

// Snaps to day 1 (month) or Jan 1 (year) before shifting -- setMonth/
// setFullYear roll an out-of-range day-of-month into the *next* month
// (e.g. 31 March minus 1 month lands on 3 March, not the intended
// end-of-February), and a period doesn't care which day-of-month its own
// anchor carries anyway, so normalizing first sidesteps the overflow
// entirely rather than working around it after the fact.
function shiftPeriod(period: Period, anchor: number, direction: 1 | -1): number {
  const date = new Date(anchor * 1000);
  switch (period) {
    case "day":
      date.setDate(date.getDate() + direction);
      break;
    case "week":
      date.setDate(date.getDate() + direction * 7);
      break;
    case "month":
      date.setDate(1);
      date.setMonth(date.getMonth() + direction);
      break;
    case "year":
      date.setMonth(0, 1);
      date.setFullYear(date.getFullYear() + direction);
      break;
    case "5year":
      date.setMonth(0, 1);
      date.setFullYear(date.getFullYear() + direction * 5);
      break;
  }
  return Math.floor(date.getTime() / 1000);
}

// Whether `anchor`'s period-window already contains `now` -- used to
// disable "next" and the "jump to now" button, since there's nothing
// beyond the current period to navigate to (no future data exists).
function isCurrentPeriod(period: Period, anchor: number, now: number): boolean {
  const a = new Date(anchor * 1000);
  const n = new Date(now * 1000);
  switch (period) {
    case "day":
      return a.getFullYear() === n.getFullYear() && a.getMonth() === n.getMonth() && a.getDate() === n.getDate();
    case "week": {
      const mondayStart = (d: Date) => {
        const copy = new Date(d);
        const daysSinceMonday = (copy.getDay() + 6) % 7;
        copy.setDate(copy.getDate() - daysSinceMonday);
        copy.setHours(0, 0, 0, 0);
        return copy.getTime();
      };
      return mondayStart(a) === mondayStart(n);
    }
    case "month":
      return a.getFullYear() === n.getFullYear() && a.getMonth() === n.getMonth();
    case "year":
    case "5year":
      return a.getFullYear() === n.getFullYear();
  }
}

interface PeriodSelectorProps {
  period: Period;
  anchor: number;
  onSelect: (period: Period, anchor: number) => void;
}

/** Owns both prev/current/next navigation through the selected period and,
 * to its right, the period-type dropdown (day/week/month/year/5year).
 * Switching period keeps the same underlying anchor instant -- e.g.
 * viewing "3 augustus" in Dag and switching to Maand lands on augustus
 * 2026, not necessarily the current month. */
export function PeriodSelector({ period, anchor, onSelect }: PeriodSelectorProps) {
  const now = Math.floor(Date.now() / 1000);
  const isCurrent = isCurrentPeriod(period, anchor, now);

  return (
    <>
      <div className="period-selector">
        <button type="button" aria-label="Vorige periode" onClick={() => onSelect(period, shiftPeriod(period, anchor, -1))}>
          ◀
        </button>
        <button type="button" className="period-current-button" disabled={isCurrent} onClick={() => onSelect(period, now)}>
          {CURRENT_LABEL[period]}
        </button>
        <button
          type="button"
          aria-label="Volgende periode"
          disabled={isCurrent}
          onClick={() => onSelect(period, shiftPeriod(period, anchor, 1))}
        >
          ▶
        </button>
      </div>
      <PeriodTypeMenu period={period} onSelect={(nextPeriod) => onSelect(nextPeriod, anchor)} />
    </>
  );
}
