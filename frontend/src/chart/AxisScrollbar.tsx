import { useRef } from "react";
import { panRange, rangeSpan, resizeRangeEdge, type NumRange } from "./range";

interface AxisScrollbarProps {
  orientation: "horizontal" | "vertical";
  domain: NumRange;
  range: NumRange;
  onChange: (next: NumRange) => void;
  onPage: (direction: 1 | -1) => void;
}

/** A classic OS-style scrollbar (track + pill thumb + two edge handles),
 * driving a continuous [start, end] value window instead of Recharts'
 * index-based Brush -- Recharts has no vertical-brush equivalent at all,
 * and reworking the x-axis to a fixed time-domain (see useAxisZoom) means
 * it can no longer be index-based either, so both axes share this one
 * implementation instead of one Recharts-Brush + one bespoke component.
 *
 * Three interactive zones: the thumb pans (drag), the two handles resize
 * just their own edge (drag), and the bare track behind the thumb pages
 * by one screen (click) -- exactly what a real scrollbar's track does. */
export function AxisScrollbar({ orientation, domain, range, onChange, onPage }: AxisScrollbarProps) {
  const trackRef = useRef<HTMLDivElement>(null);
  const horizontal = orientation === "horizontal";

  function valueAtClientPos(clientPos: number): number {
    const rect = trackRef.current!.getBoundingClientRect();
    const trackLength = horizontal ? rect.width : rect.height;
    const fraction = horizontal ? (clientPos - rect.left) / trackLength : (clientPos - rect.top) / trackLength;
    const domainSpan = rangeSpan(domain);
    // Vertical's track top is the axis's *largest* value (charts read
    // bottom-to-top), so the fraction runs the opposite way from
    // horizontal's straightforward left-to-right domain.start -> domain.end.
    return horizontal ? domain.start + fraction * domainSpan : domain.end - fraction * domainSpan;
  }

  function clientPos(e: PointerEvent | React.PointerEvent): number {
    return horizontal ? e.clientX : e.clientY;
  }

  function handleTrackClick(e: React.MouseEvent) {
    if (e.target !== trackRef.current) return; // thumb/handles stopPropagation their own pointerdown
    const clicked = valueAtClientPos(horizontal ? e.clientX : e.clientY);
    onPage(clicked < range.start ? -1 : 1);
  }

  function startPan(e: React.PointerEvent) {
    e.stopPropagation();
    const rect = trackRef.current!.getBoundingClientRect();
    const trackLength = horizontal ? rect.width : rect.height;
    const startPos = clientPos(e);
    const startRange = range;

    function onMove(ev: PointerEvent) {
      const deltaPx = clientPos(ev) - startPos;
      const deltaValue = (deltaPx / trackLength) * rangeSpan(domain) * (horizontal ? 1 : -1);
      onChange(panRange(startRange, domain, deltaValue));
    }
    function onUp() {
      window.removeEventListener("pointermove", onMove);
      window.removeEventListener("pointerup", onUp);
    }
    window.addEventListener("pointermove", onMove);
    window.addEventListener("pointerup", onUp);
  }

  function startResize(edge: "start" | "end") {
    return (e: React.PointerEvent) => {
      e.stopPropagation();
      function onMove(ev: PointerEvent) {
        onChange(resizeRangeEdge(range, domain, edge, valueAtClientPos(clientPos(ev))));
      }
      function onUp() {
        window.removeEventListener("pointermove", onMove);
        window.removeEventListener("pointerup", onUp);
      }
      window.addEventListener("pointermove", onMove);
      window.addEventListener("pointerup", onUp);
    };
  }

  const domainSpan = rangeSpan(domain) || 1;
  const startFraction = horizontal
    ? (range.start - domain.start) / domainSpan
    : (domain.end - range.end) / domainSpan;
  const sizeFraction = rangeSpan(range) / domainSpan;

  const thumbStyle = horizontal
    ? { left: `${startFraction * 100}%`, width: `${sizeFraction * 100}%` }
    : { top: `${startFraction * 100}%`, height: `${sizeFraction * 100}%` };

  return (
    <div className={`axis-scrollbar axis-scrollbar-${orientation}`} ref={trackRef} onClick={handleTrackClick}>
      <div className="axis-scrollbar-thumb" style={thumbStyle} onPointerDown={startPan}>
        {/* Vertical's track is inverted (top = domain.end, see
            valueAtClientPos) -- the handle nearer the top of the screen
            always drives whichever edge sits at the top, which is "end"
            for vertical and "start" for horizontal. */}
        <div
          className="axis-scrollbar-handle"
          style={horizontal ? { left: 0 } : { top: 0 }}
          onPointerDown={startResize(horizontal ? "start" : "end")}
        />
        <div
          className="axis-scrollbar-handle"
          style={horizontal ? { right: 0 } : { bottom: 0 }}
          onPointerDown={startResize(horizontal ? "end" : "start")}
        />
      </div>
    </div>
  );
}
