const TOOTH_ANGLES_DEG = [0, 45, 90, 135, 180, 225, 270, 315];
const CENTER = 10;
const RING_RADIUS = 4;
const TOOTH_WIDTH = 2;
const TOOTH_LENGTH = 2.6;

// Solid rectangular teeth flush against a ring -- lines radiating outward
// from a point (the first version of this icon) read as a sun, not a gear.
export function CogIcon() {
  const toothX = CENTER - TOOTH_WIDTH / 2;
  // -1 overlaps the tooth slightly into the ring's own edge, so there's no
  // visible gap between the two shapes at any icon size.
  const toothY = CENTER - RING_RADIUS - TOOTH_LENGTH + 1;

  return (
    <svg viewBox="0 0 20 20" width="32" height="32" aria-hidden="true">
      {TOOTH_ANGLES_DEG.map((deg) => (
        <rect
          key={deg}
          x={toothX}
          y={toothY}
          width={TOOTH_WIDTH}
          height={TOOTH_LENGTH}
          fill="currentColor"
          transform={`rotate(${deg} ${CENTER} ${CENTER})`}
        />
      ))}
      <circle cx={CENTER} cy={CENTER} r={RING_RADIUS} fill="none" stroke="currentColor" strokeWidth="2" />
      <circle cx={CENTER} cy={CENTER} r="1.3" fill="currentColor" />
    </svg>
  );
}
