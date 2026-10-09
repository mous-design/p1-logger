import { useEffect, useState } from "react";
import { pageRange, sameRange, zoomRange, type NumRange } from "./range";

const ZOOM_FACTOR = 0.5;

/** Owns the visible x- and y-range for a chart, independently pannable,
 * resizable, and zoomable per axis -- a combined "zoom both at once"
 * magnifier tried first, but turned out to read as unpredictable in
 * practice (zooming in on a value range you were about to read also
 * yanking the time window under it, and vice versa), so each axis gets
 * its own zoom in/out/reset instead.
 *
 * `domainX`/`domainY` are the full, fixed bounds (the period's own
 * [start, end) and the full period's own value range) -- resolved once
 * per data load, not recomputed from whatever happens to be currently
 * visible. That's what keeps zooming/panning stable instead of the axis
 * rescaling itself on every interaction. */
export function useAxisZoom(domainX: NumRange, domainY: NumRange) {
  const [rangeX, setRangeXState] = useState<NumRange>(domainX);
  const [rangeY, setRangeYState] = useState<NumRange>(domainY);

  // A fresh domain (new period/anchor fetch, or the y-domain finishing its
  // first computation) resets to fully zoomed out -- a stale zoom window
  // from the previous chart shouldn't carry over.
  useEffect(() => {
    setRangeXState(domainX);
  }, [domainX.start, domainX.end]);
  useEffect(() => {
    setRangeYState(domainY);
  }, [domainY.start, domainY.end]);

  function setRangeX(next: NumRange) {
    setRangeXState(next);
  }
  function setRangeY(next: NumRange) {
    setRangeYState(next);
  }
  function pageX(direction: 1 | -1) {
    setRangeXState((prev) => pageRange(prev, domainX, direction));
  }
  function pageY(direction: 1 | -1) {
    setRangeYState((prev) => pageRange(prev, domainY, direction));
  }

  function zoomInX() {
    setRangeXState((prev) => zoomRange(prev, domainX, ZOOM_FACTOR));
  }
  function zoomOutX() {
    setRangeXState((prev) => zoomRange(prev, domainX, 1 / ZOOM_FACTOR));
  }
  function resetX() {
    setRangeXState(domainX);
  }
  function zoomInY() {
    setRangeYState((prev) => zoomRange(prev, domainY, ZOOM_FACTOR));
  }
  function zoomOutY() {
    setRangeYState((prev) => zoomRange(prev, domainY, 1 / ZOOM_FACTOR));
  }
  function resetY() {
    setRangeYState(domainY);
  }

  // Mirrors zoomRange's own MIN_ZOOM_FRACTION floor -- once a range hits
  // it, a further zoomIn() would be a no-op, so the button should read as
  // disabled rather than silently doing nothing on click.
  const isXFullyZoomedOut = sameRange(rangeX, domainX);
  const isXAtMinZoom = sameRange(zoomRange(rangeX, domainX, ZOOM_FACTOR), rangeX);
  const isYFullyZoomedOut = sameRange(rangeY, domainY);
  const isYAtMinZoom = sameRange(zoomRange(rangeY, domainY, ZOOM_FACTOR), rangeY);

  return {
    domainX,
    domainY,
    rangeX,
    rangeY,
    setRangeX,
    setRangeY,
    pageX,
    pageY,
    zoomInX,
    zoomOutX,
    resetX,
    zoomInY,
    zoomOutY,
    resetY,
    canZoomInX: !isXAtMinZoom,
    canZoomOutX: !isXFullyZoomedOut,
    canResetX: !isXFullyZoomedOut,
    canZoomInY: !isYAtMinZoom,
    canZoomOutY: !isYFullyZoomedOut,
    canResetY: !isYFullyZoomedOut,
  };
}
