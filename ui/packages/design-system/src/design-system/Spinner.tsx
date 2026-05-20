import type { ComponentProps } from "react";

import { cn } from "../utils";
import { WakePulse } from "./WakePulse";

export type SpinnerSize = "sm" | "md" | "lg";

const DOT_SIZE: Record<SpinnerSize, string> = {
  sm: "h-3 w-3",
  md: "h-3.5 w-3.5",
  lg: "h-5 w-5",
};

export interface SpinnerProps extends Omit<ComponentProps<"span">, "children"> {
  /** Pulse-dot diameter. `sm` for in-button, `lg` for page-level loaders. */
  size?: SpinnerSize;
  /** Visible text beside the dot — use for standalone loaders. */
  label?: string;
  /** Screen-reader text when there is no visible `label`. */
  srLabel?: string;
}

/*
 * The system's indeterminate loading affordance. The design system has
 * exactly one animation — the wake-pulse glow ring (tokens.css
 * `[data-live]`) — so the loader is a brand pulse, not a rotating icon
 * (a spin would be a second, off-system animation). Use Spinner for
 * "working" waits: page-level loaders (with `label`) and in-button submit
 * feedback (dot only). Use Skeleton instead when the wait is "page-shape
 * pending" — a different affordance for a different kind of wait.
 */
export function Spinner({
  size = "md",
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
        "inline-flex items-center gap-2 text-muted-foreground",
        className,
      )}
      {...rest}
    >
      <WakePulse
        live
        aria-hidden="true"
        className={cn("inline-block shrink-0 rounded-full bg-pulse", DOT_SIZE[size])}
      />
      {label ? <span>{label}</span> : <span className="sr-only">{srLabel}</span>}
    </span>
  );
}
