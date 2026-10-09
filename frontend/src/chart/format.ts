export type MilliUnit = "kW" | "kWh" | "m³";

/** Every value arrives as a milli-unit integer (W, Wh, dm³ -- the int-only
 * parse path, see CLAUDE.md); this is the one place that turns one into its
 * display unit. Precision is a per-context choice (axis tick vs tooltip vs
 * headline number vs live reading), so callers pass it explicitly instead
 * of each redefining its own near-identical formatter. */
export function formatMilli(value: number, unit: MilliUnit, decimals: number): string {
  return `${(value / 1000).toFixed(decimals)} ${unit}`;
}

/** Wraps a number formatter into the shape Recharts' Tooltip `formatter`
 * expects -- its value is typed loosely (any ReactNode), even though ours
 * is always numeric data. */
export function tooltipFormatter(format: (value: number) => string) {
  return (value: unknown, name: unknown): [string, string] => [
    typeof value === "number" ? format(value) : String(value),
    String(name),
  ];
}
