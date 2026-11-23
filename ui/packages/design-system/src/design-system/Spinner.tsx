import type { ComponentProps, ReactNode } from "react";

import { cn } from "../utils";
import { BrandMark, SignalSpinner } from "./BrandMark";

export type SpinnerSize = "sm" | "md" | "lg";

export interface SpinnerProps extends Omit<ComponentProps<"span">, "children"> {
  /** Inline mark diameter. `sm` for in-button, `lg` for standalone inline waits. */
  size?: SpinnerSize;
  /** Page loading uses the relay mark above the status label. */
  variant?: "inline" | "page";
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
const RELAY_PATH = "M12 -6C15 -6 20 -2 25 4C29 9 31 15 29 20C27 24 21 28 12 28C3 28 -3 24 -5 20C-7 15 -5 9 -1 4C4 -2 9 -6 12 -6Z";

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
  return (
    <span
      role="status"
      aria-busy="true"
      className={cn(
        "inline-flex items-center",
        variant === "page" ? "flex-col gap-lg" : "gap-2",
        "text-muted-foreground",
        label &&
          "font-sans text-label leading-label",
        className,
      )}
      {...rest}
    >
      {variant === "page" ? <SignalRelay /> : <SignalSpinner className={MARK_SIZE[size]} />}
      {label ? <span>{label}</span> : <span className="sr-only">{srLabel}</span>}
    </span>
  );
}

function SignalRelay() {
  return (
    <svg viewBox="-10 -10 44 44" width={44} height={44} fill="none"
      aria-hidden="true" focusable="false" data-signal-relay=""
      className="shrink-0 text-pulse forced-colors:text-[CanvasText]">
      <path d={RELAY_PATH} strokeWidth={1.25} className="stroke-border forced-colors:stroke-[GrayText]" />
      <g className="fill-text-dim forced-colors:fill-[GrayText]">
        <circle cx={12} cy={-6} r={1.8} />
        <circle cx={29} cy={20} r={1.8} />
        <circle cx={-5} cy={20} r={1.8} />
      </g>
      <path d={RELAY_PATH} data-signal-relay-pulse="" pathLength={100} stroke="currentColor"
        strokeWidth={2.25} strokeLinecap="round" strokeDasharray="9 91" strokeDashoffset={4.5} />
      <BrandMark className="text-current" />
    </svg>
  );
}
