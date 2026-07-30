"use client";

// The countdown dial for the dashboard error boundary.
//
// A ring rather than a bar: the wait restarts on every attempt, and a bar that
// refills from zero three times reads as three unrelated loads, where a dial
// sweeping round reads as one thing being retried.
//
// The sweep is `motion-safe`-gated, so under reduced motion the ring still
// tracks the wait — it just steps per tick instead of animating between them.
// The seconds digit is the real signal; the sweep only makes the wait feel
// accounted for. The ring is decoration: the countdown is announced from the
// live region in the boundary, so a screen reader hears seconds once.

const RING_SIZE_PX = 72;
const RING_STROKE_PX = 3;
// Radius must leave the stroke room inside the box, or the ring clips against
// the viewBox edge at its thickest point.
const RING_RADIUS = RING_SIZE_PX / 2 - RING_STROKE_PX;
const RING_CIRCUMFERENCE = 2 * Math.PI * RING_RADIUS;
const RING_CENTER = RING_SIZE_PX / 2;

export function RetryCountdownRing({ progress, label }: { progress: number; label: string }) {
  const clamped = Math.min(1, Math.max(0, progress));

  return (
    <div className="relative inline-flex items-center justify-center" role="presentation">
      {/* `-rotate-90` starts the sweep at 12 o'clock; SVG circles begin at 3. */}
      <svg
        width={RING_SIZE_PX}
        height={RING_SIZE_PX}
        viewBox={`0 0 ${RING_SIZE_PX} ${RING_SIZE_PX}`}
        aria-hidden="true"
        className="-rotate-90"
      >
        <circle
          cx={RING_CENTER}
          cy={RING_CENTER}
          r={RING_RADIUS}
          fill="none"
          strokeWidth={RING_STROKE_PX}
          className="stroke-border"
        />
        <circle
          cx={RING_CENTER}
          cy={RING_CENTER}
          r={RING_RADIUS}
          fill="none"
          strokeWidth={RING_STROKE_PX}
          strokeLinecap="round"
          strokeDasharray={RING_CIRCUMFERENCE}
          strokeDashoffset={RING_CIRCUMFERENCE * (1 - clamped)}
          className="stroke-pulse motion-safe:transition-[stroke-dashoffset] motion-safe:duration-snap motion-safe:ease-snap"
          data-testid="retry-ring-progress"
        />
      </svg>
      <span className="absolute font-mono text-body-sm tabular-nums text-foreground">{label}</span>
    </div>
  );
}
