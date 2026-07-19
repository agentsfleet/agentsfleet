import type { ComponentProps, ReactNode } from "react";

import { cn } from "../utils";
import { WakePulse } from "./WakePulse";

export type SpinnerSize = "sm" | "md" | "lg";

const DOT_SIZE: Record<SpinnerSize, string> = {
  sm: "h-1.5 w-1.5",
  md: "h-2 w-2",
  lg: "h-2.5 w-2.5",
};

const ORBIT_SIZE: Record<SpinnerSize, string> = {
  sm: "h-3 w-3",
  md: "h-3.5 w-3.5",
  lg: "h-5 w-5",
};

export interface SpinnerProps extends Omit<ComponentProps<"span">, "children"> {
  /** Pulse-dot diameter. `sm` for in-button, `lg` for page-level loaders. */
  size?: SpinnerSize;
  /**
   * Visible text beside the dot — use for standalone loaders. Accepts a node so
   * callers can supply a client-rendered label (see the app's LoadingVerbLabel)
   * without this component becoming client-only itself.
   */
  label?: ReactNode;
  /** Screen-reader text when there is no visible `label`. */
  srLabel?: string;
}

/*
 * The system's indeterminate loading affordance. Use Spinner for short
 * working waits and install state chips; use Skeleton when the page shape is
 * still resolving. The outer arc is intentionally tiny and monochrome so the
 * WakePulse remains the brand signal.
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
        label &&
          "rounded-md border border-primary/40 bg-primary/10 px-sm py-xs font-mono text-label font-medium leading-label text-primary",
        className,
      )}
      {...rest}
    >
      <span
        aria-hidden="true"
        data-spinner-orbit
        className={cn("relative inline-grid shrink-0 place-items-center", ORBIT_SIZE[size])}
      >
        <span className="absolute inset-0 rounded-full border border-primary/30 border-t-primary motion-safe:animate-spin" />
        <WakePulse
          live
          className={cn("inline-block rounded-full bg-pulse", DOT_SIZE[size])}
        />
      </span>
      {label ? <span>{label}</span> : <span className="sr-only">{srLabel}</span>}
    </span>
  );
}
