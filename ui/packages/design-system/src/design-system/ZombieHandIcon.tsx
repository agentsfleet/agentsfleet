import { type ComponentProps } from "react";

type Props = {
  size?: number;
} & ComponentProps<"svg">;

export default function ZombieHandIcon({ size = 20, ...rest }: Props) {
  const colors = {
    wristFill: "var(--z-icon-zombie-wrist-fill)",
    handFill: "var(--z-icon-zombie-fill)",
    line: "var(--z-icon-zombie-line)",
  };

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
      {/* Wrist / forearm */}
      <path
        d="M24 58 c-2 0-4-1-4-3 l0-14 c0-1 1-2 2-2 l16 0 c1 0 2 1 2 2 l0 14 c0 2-2 3-4 3z"
        fill={colors.wristFill}
        stroke={colors.line}
        strokeWidth="1.2"
      />
      {/* Palm */}
      <path
        d="M20 42 c-1-2-1-5 0-7 l2-6 c1-2 3-3 5-3 l10 0 c2 0 4 1 5 3 l2 6 c1 2 1 5 0 7z"
        fill={colors.handFill}
        stroke={colors.line}
        strokeWidth="1.2"
      />
      {/* Index finger */}
      <path
        d="M22 26 l-1-12 c0-2 1-3 3-3 c2 0 3 1 3 3 l-1 12z"
        fill={colors.handFill}
        stroke={colors.line}
        strokeWidth="1.2"
      />
      {/* Middle finger */}
      <path
        d="M27 26 l0-16 c0-2 1-3 3-3 c2 0 3 1 3 3 l0 16z"
        fill={colors.handFill}
        stroke={colors.line}
        strokeWidth="1.2"
      />
      {/* Ring finger */}
      <path
        d="M34 26 l1-14 c0-2 1-3 3-3 c2 0 3 1 3 3 l-1 14z"
        fill={colors.handFill}
        stroke={colors.line}
        strokeWidth="1.2"
      />
      {/* Pinky finger */}
      <path
        d="M40 28 l2-10 c0-2 1-3 3-3 c1.5 0 2.5 1 2.5 3 l-2 10z"
        fill={colors.handFill}
        stroke={colors.line}
        strokeWidth="1.2"
      />
      {/* Thumb */}
      <path
        d="M20 36 l-6-4 c-2-1-2-3-1-4.5 c1-1.5 3-1.5 4.5-0.5 l5 4z"
        fill={colors.handFill}
        stroke={colors.line}
        strokeWidth="1.2"
      />
      {/* Knuckle details */}
      <circle cx="25" cy="28" r="1" fill={colors.line} opacity="0.4" />
      <circle cx="30" cy="27" r="1" fill={colors.line} opacity="0.4" />
      <circle cx="36" cy="28" r="1" fill={colors.line} opacity="0.4" />
      {/* Stitches on wrist */}
      <line x1="27" y1="46" x2="27" y2="50" stroke={colors.line} strokeWidth="0.8" opacity="0.5" />
      <line x1="32" y1="45" x2="32" y2="49" stroke={colors.line} strokeWidth="0.8" opacity="0.5" />
      <line x1="37" y1="46" x2="37" y2="50" stroke={colors.line} strokeWidth="0.8" opacity="0.5" />
    </svg>
  );
}
