import { type ComponentProps } from "react";

import { cn } from "../utils";

export type StatusPillVariant = "neutral" | "pulse" | "success" | "warning" | "danger";

export type StatusPillProps = ComponentProps<"span"> & {
  variant?: StatusPillVariant;
  dot?: boolean;
};

const pillClass: Record<StatusPillVariant, string> = {
  neutral: "border-border bg-secondary text-muted-foreground",
  pulse: "border-primary/40 bg-primary/10 text-primary",
  success: "border-success/40 bg-success/10 text-success",
  warning: "border-warning/40 bg-warning/10 text-warning",
  danger: "border-destructive/40 bg-destructive/10 text-destructive",
};

const dotClass: Record<StatusPillVariant, string> = {
  neutral: "bg-muted-foreground",
  pulse: "bg-primary",
  success: "bg-success",
  warning: "bg-warning",
  danger: "bg-destructive",
};

export function StatusPill({
  variant = "neutral",
  dot = false,
  className,
  children,
  ref,
  ...props
}: StatusPillProps) {
  return (
    <span
      ref={ref}
      data-variant={variant}
      className={cn(
        "inline-flex items-center gap-1.5 rounded-sm border px-2 py-0.5",
        "font-mono text-label font-medium uppercase leading-label tracking-label",
        pillClass[variant],
        className,
      )}
      {...props}
    >
      {dot ? <span className={cn("h-1.5 w-1.5 rounded-full", dotClass[variant])} /> : null}
      {children}
    </span>
  );
}
