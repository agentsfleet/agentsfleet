import { type HTMLAttributes, type ReactNode } from "react";
import { cn } from "../utils";

/*
 * UsageBar — a quota/usage meter: track + solid fill, an optional
 * label + tabular-nums percentage row, and an optional sub-caption.
 * Extracted from the bespoke `.app-meter` markup BillingBalanceCard
 * previously hand-rolled. The component owns its solid fill and typography
 * so consumers need no accompanying stylesheet.
 * `label` is optional — BillingBalanceCard's meter was
 * always unlabeled/aria-hidden (the dollar headline above it already
 * states the value), so the label+percentage row only renders when a
 * caller opts in. RSC-safe, no asChild — mirrors StatusCard's shape.
 */
export interface UsageBarProps extends HTMLAttributes<HTMLDivElement> {
  label?: string;
  pct: number;
  sublabel?: ReactNode;
}

export function UsageBar({ label, pct, sublabel, className, ...rest }: UsageBarProps) {
  const clamped = Math.min(100, Math.max(0, pct));

  return (
    <div
      data-slot="usage-bar"
      data-testid="usage-bar"
      className={cn("flex flex-col gap-2 font-sans", className)}
      {...rest}
    >
      {label ? (
        <div className="flex items-baseline justify-between gap-2 text-sm">
          <span className="text-foreground">{label}</span>
          <span className="tabular-nums text-muted-foreground">{clamped}%</span>
        </div>
      ) : null}
      <div className="usage-bar-track h-2 overflow-hidden rounded-full bg-accent" aria-hidden="true">
        <span className="usage-bar-fill block h-full rounded-full bg-pulse" style={{ width: `${clamped}%` }} />
      </div>
      {sublabel ? (
        <div className="text-xs text-muted-foreground">{sublabel}</div>
      ) : null}
    </div>
  );
}
