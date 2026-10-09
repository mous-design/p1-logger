import type { ReactNode } from "react";

interface WidgetTileProps {
  title: string;
  onOpen: () => void;
  loading: boolean;
  error: string | null;
  children: ReactNode;
}

/** Shared chrome for a dashboard tile -- title + a click-to-open-detail
 * button, no other controls. Zoom/legend/period-selectors etc. are only
 * ever in the detail view, per the "widget toont alleen een graph" design. */
export function WidgetTile({ title, onOpen, loading, error, children }: WidgetTileProps) {
  return (
    <button type="button" className="widget-tile" onClick={onOpen}>
      <div className="widget-tile-title">{title}</div>
      {loading && <p className="widget-tile-status">Laden…</p>}
      {error && <p className="widget-tile-status">Fout: {error}</p>}
      {!loading && !error && children}
    </button>
  );
}
