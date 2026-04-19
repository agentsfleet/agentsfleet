import { cva, type VariantProps } from "class-variance-authority";
import { type ComponentProps } from "react";
import { cn } from "../utils";

/*
 * Badge — pill label with semantic status variants. RSC-safe, React 19
 * ref-as-prop. Variants map to Layer 2 semantic tokens via Tailwind v4
 * opacity modifiers, so a theme-level color change propagates automatically.
 */
export const badgeVariants = cva(
  [
    "inline-flex items-center gap-1.5 rounded-full border px-2.5 py-0.5",
    "font-mono text-[0.7rem] font-medium uppercase tracking-wider transition-colors",
  ].join(" "),
  {
    variants: {
      variant: {
        default: "border-border bg-muted text-muted-foreground",
        orange: "border-primary/30 bg-primary/10 text-primary",
        amber: "border-warning/20 bg-warning/10 text-warning",
        green: "border-success/20 bg-success/10 text-success",
        cyan: "border-info/20 bg-info/10 text-info",
        destructive:
          "border-destructive/20 bg-destructive/10 text-destructive",
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
