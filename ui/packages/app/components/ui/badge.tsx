import * as React from "react";
import { cva, type VariantProps } from "class-variance-authority";
import { cn } from "@/lib/utils";

const badgeVariants = cva(
  "inline-flex items-center gap-1.5 rounded-full border px-2.5 py-0.5 font-mono text-[0.7rem] font-medium uppercase tracking-wider transition-colors",
  {
    variants: {
      variant: {
        default:     "border-[var(--z-border)] bg-[var(--z-surface-1)] text-[var(--z-text-muted)]",
        orange:      "border-[rgba(255,107,53,0.3)] bg-[rgba(255,107,53,0.1)] text-[var(--z-orange)]",
        amber:       "border-[rgba(255,190,46,0.2)] bg-[rgba(255,190,46,0.08)] text-[var(--z-amber)]",
        green:       "border-[rgba(57,255,133,0.2)] bg-[rgba(57,255,133,0.08)] text-[var(--z-green)]",
        cyan:        "border-[rgba(94,212,236,0.2)] bg-[rgba(94,212,236,0.08)] text-[var(--z-cyan)]",
        destructive: "border-[rgba(255,77,106,0.2)] bg-[rgba(255,77,106,0.08)] text-[var(--z-red)]",
      },
    },
    defaultVariants: { variant: "default" },
  },
);

export interface BadgeProps
  extends React.HTMLAttributes<HTMLDivElement>,
    VariantProps<typeof badgeVariants> {}

function Badge({ className, variant, ...props }: BadgeProps) {
  return (
    <div className={cn(badgeVariants({ variant }), className)} {...props} />
  );
}

export { Badge, badgeVariants };
