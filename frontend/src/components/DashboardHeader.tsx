import { usePopupMenu } from "../hooks/usePopupMenu";
import { CogIcon } from "./CogIcon";
import type { Widget, WidgetId } from "../types";

interface DashboardHeaderProps {
  widgets: Widget[];
  enabled: Record<WidgetId, boolean>;
  onToggle: (id: WidgetId) => void;
}

export function DashboardHeader({ widgets, enabled, onToggle }: DashboardHeaderProps) {
  const { open: menuOpen, setOpen: setMenuOpen, menuRef } = usePopupMenu();

  return (
    <header className="dashboard-header">
      <h1>Energie dashboard</h1>
      <div className="dashboard-header-menu" ref={menuRef}>
        <button type="button" className="cog-button" aria-label="Widgets aan/uit" onClick={() => setMenuOpen((open) => !open)}>
          <CogIcon />
        </button>
        {menuOpen && (
          <div className="widget-toggle-menu">
            {/* Its own section title, not just a bare checkbox list -- room
                to add other, unrelated settings under this same cog later
                without the widget toggles looking like the whole menu. */}
            <div className="menu-section-title">Toon widgets</div>
            {widgets.map((w) => (
              <label key={w.id} className="widget-toggle-item">
                <input type="checkbox" checked={enabled[w.id]} onChange={() => onToggle(w.id)} />
                {w.label}
              </label>
            ))}
          </div>
        )}
      </div>
    </header>
  );
}
