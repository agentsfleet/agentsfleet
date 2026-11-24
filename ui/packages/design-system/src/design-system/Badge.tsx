import { Slot } from "@radix-ui/react-slot";
import { cva, type VariantProps } from "class-variance-authority";
import { type ComponentProps } from "react";
import { cn } from "../utils";

const NEUTRAL_BADGE = "border-border bg-transparent text-muted-foreground";

export const badgeVariants = cva(
  [
    "inline-flex items-center gap-1.5 rounded-chip border px-2 py-1",
    "font-sans text-eyebrow font-normal leading-eyebrow transition-colors",
  ].join(" "),
  {
    variants: {
      variant: {
        default: NEUTRAL_BADGE,
        count: `${NEUTRAL_BADGE} min-w-6 shrink-0 justify-center tabular-nums`,
        orange: "border-border bg-transparent text-foreground",
        amber: "border-border bg-transparent text-warning",
        green: "border-border bg-transparent text-success",
        cyan: NEUTRAL_BADGE,
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
  VariantProps<typeof badgeVariants> & {
    asChild?: boolean;
  };

export function Badge({ asChild, className, variant, ref, ...props }: BadgeProps) {
  const Comp = asChild ? Slot : "div";
  return (
    <Comp
      ref={ref}
      className={cn(badgeVariants({ variant }), className)}
      {...props}
    />
  );
}

export default Badge;
