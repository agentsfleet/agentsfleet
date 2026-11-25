import type { ComponentProps, CSSProperties, ReactNode } from "react";

import { cn } from "../utils";

export type SpinnerSize = "sm" | "md" | "lg";

export interface SpinnerProps extends Omit<ComponentProps<"span">, "children"> {
  /** Inline mark diameter. `sm` for in-button, `lg` for standalone inline waits. */
  size?: SpinnerSize;
  /** Full-screen waits also display the wordmark beside the forming brand mark. */
  variant?: "inline" | "page" | "full-screen";
  /**
   * Visible loading label. The page variant places it below the mark.
   */
  label?: ReactNode;
  /** Screen-reader text when there is no visible `label`. */
  srLabel?: string;
}

const MARK_SIZE: Record<SpinnerSize, string> = {
  sm: "size-3",
  md: "size-4",
  lg: "size-6",
};
// Stroke midpoints in SVG units, relative to the brand mark's fixed center.
const STROKE_PAIRS = [
  {startY: -6, rayX: 0, rayY: -7, angle: 90},
  {startY: -2, rayX: -4.95, rayY: -4.95, angle: 45},
  {startY: 2, rayX: -7, rayY: 0, angle: 0},
  {startY: 6, rayX: -4.95, rayY: 4.95, angle: -45},
];

/*
 * The system's indeterminate loading affordance. Use Spinner for short
 * working waits and install state chips; use Skeleton when the page shape is
 * still resolving. Motion lives in CSS; the mark needs no client runtime.
 */
export function Spinner({
  size = "md",
  variant = "inline",
  label,
  srLabel = "Loading",
  className,
  ...rest
}: SpinnerProps) {
  const inline = variant === "inline";
  return (
    <span
      role="status"
      aria-busy="true"
      className={cn(
        "inline-flex items-center",
        inline ? "gap-2" : "flex-col gap-lg",
        "text-muted-foreground",
        label &&
          "font-sans text-label leading-label",
        className,
      )}
      {...rest}
    >
      {inline ? <LoadingMark className={MARK_SIZE[size]} /> :
        <span aria-hidden="true" className="inline-flex items-center gap-md">
          <LoadingMark />
          {variant === "full-screen" && <span className="text-wordmark leading-none font-title text-foreground">agentsfleet</span>}
        </span>}
      {label ? <span>{label}</span> : <span className="sr-only">{srLabel}</span>}
    </span>
  );
}

function LoadingMark({ className }: { className?: string }) {
  return <svg viewBox="0 0 30 30" width={60} height={60} fill="none" aria-hidden="true" focusable="false"
    className={cn("shrink-0 text-pulse forced-colors:text-[CanvasText]", className)}>
    <g transform="translate(15 15)" stroke="currentColor" strokeWidth={1.4} strokeLinecap="round">
      {STROKE_PAIRS.map(({startY, rayX, rayY, angle}, index) => [1, -1].map(side => {
        const style: CSSProperties & Record<`--${string}`, string> = {
          "--start-x": `${-4 * side}px`, "--start-y": `${startY * side}px`,
          "--ray-x": `${rayX * side}px`, "--ray-y": `${rayY * side}px`,
          "--ray-angle": `${angle}deg`, "--delay": `${index * 45}ms`,
        };
        return <path key={`${startY}:${side}`} className="page-loading-stroke" d="M-3 0h6" style={style} />;
      }))}
      <circle r={1.5} fill="currentColor" stroke="none" />
    </g>
  </svg>;
}
