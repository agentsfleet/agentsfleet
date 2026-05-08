import { type ComponentProps, useId } from "react";

type Props = {
  size?: number;
} & ComponentProps<"svg">;

/*
 * Brand glyph. Mostly silhouette via `currentColor` so the icon adapts
 * to whatever text color the consumer passes in (header, footer, hero
 * sit on different surfaces). A subtle --pulse-dim → --pulse gradient
 * fills the palm + fingers, so the brand-mark carries a hint of the
 * signature accent without crossing into decoration.
 */
export default function ZombieHandIcon({ size = 20, ...rest }: Props) {
  // Per-instance gradient id so multiple icons on one page don't share a
  // <linearGradient> def — without this, browsers honour the first defs
  // and the second icon paints flat.
  const gradientId = `z-hand-grad-${useId()}`;
  const handFill = `url(#${gradientId})`;

  return (
    <svg
      xmlns="http://www.w3.org/2000/svg"
      viewBox="0 0 64 64"
      width={size}
      height={size}
      fill="none"
      aria-hidden="true"
      {...rest}
    >
      <defs>
        <linearGradient id={gradientId} x1="0" y1="0" x2="1" y2="1">
          <stop offset="0%" stopColor="var(--pulse-dim)" />
          <stop offset="100%" stopColor="var(--pulse)" />
        </linearGradient>
      </defs>
      {/* Wrist / forearm — currentColor at reduced opacity so the
       * silhouette stays readable on any container. */}
      <path
        d="M21 59c-3 0-5-2-5-5V41c0-2 1-3 3-3h20c2 0 3 1 3 3v13c0 3-2 5-5 5z"
        fill="currentColor"
        fillOpacity="0.55"
        stroke="currentColor"
        strokeOpacity="0.7"
        strokeWidth="1.2"
      />
      {/* Palm — pulse-dim → pulse gradient */}
      <path
        d="M19 41c-1-3-1-7 1-10l2-5c1-2 3-3 5-3h10c3 0 5 1 6 4l2 5c1 3 1 6 0 9l-1 3H20z"
        fill={handFill}
        stroke="currentColor"
        strokeOpacity="0.7"
        strokeWidth="1.2"
      />
      {/* Index finger */}
      <path
        d="M21 25V15c0-3 2-5 4-5s4 2 4 5v13l-2 6h-4l-2-5z"
        fill={handFill}
        stroke="currentColor"
        strokeOpacity="0.7"
        strokeWidth="1.2"
      />
      {/* Middle finger */}
      <path
        d="M28 23V11c0-3 2-5 4-5s4 2 4 5v14l-2 7h-4l-2-6z"
        fill={handFill}
        stroke="currentColor"
        strokeOpacity="0.7"
        strokeWidth="1.2"
      />
      {/* Ring finger */}
      <path
        d="M36 26l1-11c0-3 2-5 4-5s4 2 4 5l-1 12-3 6h-3l-2-5z"
        fill={handFill}
        stroke="currentColor"
        strokeOpacity="0.7"
        strokeWidth="1.2"
      />
      {/* Pinky finger */}
      <path
        d="M44 31l2-8c1-2 2-4 4-4c2 0 3 2 3 4l-2 9-3 4h-3l-1-4z"
        fill={handFill}
        stroke="currentColor"
        strokeOpacity="0.7"
        strokeWidth="1.2"
      />
      {/* Thumb */}
      <path
        d="M20 39l-7-4c-2-1-3-3-2-5c1-2 4-2 6 0l6 4-1 5z"
        fill={handFill}
        stroke="currentColor"
        strokeOpacity="0.7"
        strokeWidth="1.2"
      />
      {/* Nails — muted text colour */}
      <path d="M23 10l2-4 2 4z" fill="currentColor" fillOpacity="0.4" />
      <path d="M30 6l2-4 2 4z" fill="currentColor" fillOpacity="0.4" />
      <path d="M39 10l2-4 2 4z" fill="currentColor" fillOpacity="0.4" />
      <path d="M48 19l2-4 1 4z" fill="currentColor" fillOpacity="0.4" />
      <path d="M11 31l-4-1 2-3z" fill="currentColor" fillOpacity="0.4" />
      {/* Knuckle details */}
      <circle cx="25" cy="30" r="1" fill="currentColor" opacity="0.4" />
      <circle cx="32" cy="28" r="1" fill="currentColor" opacity="0.4" />
      <circle cx="39" cy="30" r="1" fill="currentColor" opacity="0.4" />
      <path d="M24 45h10" stroke="currentColor" strokeWidth="1" opacity="0.35" />
      {/* Stitches on wrist */}
      <line x1="26" y1="47" x2="26" y2="52" stroke="currentColor" strokeWidth="0.8" opacity="0.5" />
      <line x1="32" y1="46" x2="32" y2="51" stroke="currentColor" strokeWidth="0.8" opacity="0.5" />
      <line x1="38" y1="47" x2="38" y2="52" stroke="currentColor" strokeWidth="0.8" opacity="0.5" />
    </svg>
  );
}
