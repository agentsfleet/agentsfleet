import { cva, type VariantProps } from "class-variance-authority";
import { type ComponentProps } from "react";
import { cn } from "../utils";

export const badgeVariants = cva(
  [
    "inline-flex items-center gap-1.5 rounded-chip border px-2 py-1",
    "font-sans text-eyebrow font-normal leading-eyebrow transition-colors",
  ].join(" "),
  {
    variants: {
      variant: {
        default: "border-border bg-transparent text-muted-foreground",
        orange: "border-border bg-transparent text-foreground",
        amber: "border-border bg-transparent text-warning",
        green: "border-border bg-transparent text-success",
        cyan: "border-border bg-transparent text-muted-foreground",
        destructive:
          "border-border bg-transparent text-destructive",
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
