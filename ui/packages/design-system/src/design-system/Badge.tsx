import { cva, type VariantProps } from "class-variance-authority";
import { type ComponentProps } from "react";
import { cn } from "../utils";

export const badgeVariants = cva(
  [
    "inline-flex items-center gap-1.5 rounded-sm border px-2 py-0.5",
    "font-sans text-label font-semibold uppercase leading-label tracking-label transition-colors",
  ].join(" "),
  {
    variants: {
      variant: {
        default: "border-border bg-muted text-muted-foreground",
        orange: "border-primary/30 bg-primary/10 text-primary",
        amber: "border-warning/40 bg-warning/10 text-warning",
        green: "border-success/40 bg-success/10 text-success",
        cyan: "border-info/40 bg-info/10 text-info",
        destructive:
          "border-destructive/40 bg-destructive/10 text-destructive",
        live: "border-pulse bg-pulse text-on-pulse",
        warn: "border-warning bg-warning text-on-pulse",
        error: "border-destructive bg-destructive text-on-pulse",
        evidence:
          "border-evidence/40 bg-evidence/10 text-evidence",
      },
    },
    defaultVariants: { variant: "default" },
  },
);

export type BadgeVariant = NonNullable<VariantProps<typeof badgeVariants>["variant"]>;

export type BadgeProps = ComponentProps<"div"> &
  VariantProps<typeof badgeVariants>;

export function Badge({ className, variant, ref, ...props }: BadgeProps) {
  return (
    <div
      ref={ref}
      className={cn(badgeVariants({ variant }), className)}
      {...props}
    />
  );
}

export default Badge;
