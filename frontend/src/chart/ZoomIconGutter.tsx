import { MagnifierIcon } from "./MagnifierIcon";

interface ZoomIconGutterProps {
  className: string;
  canZoomIn: boolean;
  canZoomOut: boolean;
  canReset: boolean;
  onZoomIn: () => void;
  onZoomOut: () => void;
  onReset: () => void;
}

/** One axis's zoom in/out/reset trio -- rendered twice per chart (see
 * PowerChart.tsx etc.), once wired to the x-axis and once to the y-axis,
 * since a combined "zoom both at once" button turned out to read as
 * unpredictable in practice (see useAxisZoom's own doc comment). `className`
 * carries the caller's own positioning/layout (row, bottom-left, next to
 * the horizontal scrollbar vs. column, right side, centered on the
 * vertical one) -- this component only knows about the three buttons. */
export function ZoomIconGutter({ className, canZoomIn, canZoomOut, canReset, onZoomIn, onZoomOut, onReset }: ZoomIconGutterProps) {
  return (
    <div className={className}>
      <button type="button" className="zoom-icon-button" aria-label="Inzoomen" disabled={!canZoomIn} onClick={onZoomIn}>
        <MagnifierIcon symbol="plus" />
      </button>
      <button type="button" className="zoom-icon-button" aria-label="Uitzoomen" disabled={!canZoomOut} onClick={onZoomOut}>
        <MagnifierIcon symbol="minus" />
      </button>
      <button type="button" className="zoom-icon-button" aria-label="Zoom resetten" disabled={!canReset} onClick={onReset}>
        <MagnifierIcon />
      </button>
    </div>
  );
}
