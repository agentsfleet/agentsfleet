import { type HTMLAttributes, type ReactNode } from "react";
import { cn } from "../utils";

/*
 * UsageBar — a quota/usage meter: track + gradient fill, an optional
 * label + tabular-nums percentage row, and an optional sub-caption.
 * Extracted from the bespoke `.app-meter` markup BillingBalanceCard
 * previously hand-rolled; the mount-fill animation + reduced-motion
 * carve-out live in globals.css under the `usage-bar-*` class hooks below.
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
      className={cn("flex flex-col gap-2", className)}
      {...rest}
    >
      {label ? (
        <div className="flex items-baseline justify-between gap-2 text-sm">
          <span className="text-foreground">{label}</span>
          <span className="font-mono tabular-nums text-muted-foreground">{clamped}%</span>
        </div>
      ) : null}
      <div className="usage-bar-track h-2 rounded-full bg-accent" aria-hidden="true">
        <span className="usage-bar-fill block h-full rounded-full" style={{ width: `${clamped}%` }} />
      </div>
      {sublabel ? (
        <div className="font-mono text-xs text-muted-foreground">{sublabel}</div>
      ) : null}
    </div>
  );
}
