import type { ComponentProps, ReactNode } from "react";

import { cn } from "../utils";
import { SignalSpinner } from "./BrandMark";

export type SpinnerSize = "sm" | "md" | "lg";

const MARK_SIZE: Record<SpinnerSize, string> = {
  sm: "size-3",
  md: "size-4",
  lg: "size-6",
};

export interface SpinnerProps extends Omit<ComponentProps<"span">, "children"> {
  /** Mark diameter. `sm` for in-button, `lg` for standalone waits. */
  size?: SpinnerSize;
  /**
   * Visible text beside the mark — use for standalone loaders.
   */
  label?: ReactNode;
  /** Screen-reader text when there is no visible `label`. */
  srLabel?: string;
}

/*
 * The system's indeterminate loading affordance. Use Spinner for short
 * working waits and install state chips; use Skeleton when the page shape is
 * still resolving. Motion lives in CSS; the mark needs no client runtime.
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
          "font-sans text-label leading-label",
        className,
      )}
      {...rest}
    >
      <SignalSpinner className={MARK_SIZE[size]} />
      {label ? <span>{label}</span> : <span className="sr-only">{srLabel}</span>}
    </span>
  );
}
