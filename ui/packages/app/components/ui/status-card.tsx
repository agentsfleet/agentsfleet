import * as React from "react";
import { cn } from "@usezombie/design-system/utils";

// StatusCard renders its own `<dl>` internal markup, which is incompatible
// with Radix Slot's single-child clone model. For a clickable status tile,
// wrap the whole card in <Link> externally — same DOM cost, no forced slot
// pattern. `asChild` belongs on leaf primitives (Button, Badge), not on
// display compositions with internal structure.

export type StatusCardVariant = "default" | "success" | "warning" | "danger" | "muted";

export interface StatusCardProps extends React.HTMLAttributes<HTMLDivElement> {
  label: string;
  count: number | string;
  variant?: StatusCardVariant;
  trend?: "up" | "down" | "flat";
  sublabel?: string;
}

const variantAccent: Record<StatusCardVariant, string> = {
  default: "text-foreground",
  success: "text-success",
  warning: "text-warning",
  danger:  "text-destructive",
  muted:   "text-muted-foreground",
};

const trendGlyph = { up: "↑", down: "↓", flat: "→" } as const;
const trendLabel = { up: "increasing", down: "decreasing", flat: "unchanged" } as const;

export function StatusCard({
  label,
  count,
  variant = "default",
  trend,
  sublabel,
  className,
  ...rest
}: StatusCardProps) {
  const ariaLabel = [
    label,
    String(count),
    trend ? trendLabel[trend] : null,
    sublabel,
  ].filter(Boolean).join(", ");
  return (
    <div
      data-slot="status-card"
      data-testid="status-card"
      data-variant={variant}
      role="group"
      aria-label={ariaLabel}
      className={cn(
        "flex min-w-0 flex-col gap-1 rounded-md border border-border bg-card p-4 transition-colors",
        "hover:border-primary/40 focus-within:border-primary",
        "motion-reduce:transition-none",
        className,
      )}
      {...rest}
    >
      <dl className="flex min-w-0 flex-col gap-1">
        <dt className="truncate text-xs font-medium uppercase tracking-wide text-muted-foreground">
          {label}
        </dt>
        <dd className={cn("text-xl sm:text-2xl font-semibold tabular-nums", variantAccent[variant])}>
          <span>{count}</span>
          {trend ? (
            <span className="ml-1 text-base" aria-hidden="true">
              {trendGlyph[trend]}
            </span>
          ) : null}
        </dd>
        {sublabel ? (
          <dd className="truncate text-xs text-muted-foreground">{sublabel}</dd>
        ) : null}
      </dl>
    </div>
  );
}
