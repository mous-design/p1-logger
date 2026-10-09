// Matches the AxisScrollbar's own fixed width in the vertical orientation
// (see App.scss's --scrollbar-thickness) -- the empty gutter to its left
// where the zoom icons live is sized to match.
export const Y_AXIS_WIDTH = 82;
// Applied to both axes -- Recharts' default tick font-size otherwise makes
// the x-axis noticeably larger than the y-axis's own (which was shrunk to
// help the minus-sign clipping above), an inconsistency with no reason to
// exist once one axis was already being overridden.
export const AXIS_TICK_FONT_SIZE = 11;

// Shared margin for every chart's <LineChart> -- `right` reserves room for
// both the rightmost x-axis tick's centered text (Recharts' own 5px
// default isn't enough, its right half spills past the plot) *and* the
// vertical scrollbar + its own zoom-icon column that now live in that
// same margin -- see App.scss's $plot-right-inset, which must match this
// exactly; `left` is the equivalent idea mirrored for the y-axis, extra
// buffer beyond Y_AXIS_WIDTH itself.
export const CHART_MARGIN = { top: 5, right: 48, bottom: 5, left: 10 };

// Recharts' own default XAxis height (confirmed against the rendered SVG),
// now pinned explicitly instead of left to its auto layout -- the
// scrollbars in App.scss are positioned against the plot rect's exact
// pixel bounds, which only holds still if this can't silently change.
export const X_AXIS_HEIGHT = 30;
