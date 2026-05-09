import { cva, type VariantProps } from "class-variance-authority";
import { type ComponentProps } from "react";
import { cn } from "../utils";

/*
 * Badge — flat status label. Mono, --r-sm radius (spec §Component
 * principles: status badges get colored fills; informational badges
 * get muted outlines). RSC-safe, React 19 ref-as-prop. Variants map
 * to Layer 2 semantic tokens via Tailwind v4 opacity modifiers, so
 * a theme-level colour change propagates automatically.
 */
export const badgeVariants = cva(
  [
    "inline-flex items-center gap-1.5 rounded-sm border px-2 py-0.5",
    "font-mono text-label font-medium uppercase tracking-label transition-colors",
  ].join(" "),
  {
    variants: {
      variant: {
        // ── Outline tier ──────────────────────────────────────────
        // Status badges that read as informational rather than
        // assertive: low-saturation fill + tinted border + saturated
        // text. Cheap to drop into a dense list without the badge
        // dominating the row.
        default: "border-border bg-muted text-muted-foreground",
        orange: "border-primary/30 bg-primary/10 text-primary",
        amber: "border-warning/20 bg-warning/10 text-warning",
        green: "border-success/20 bg-success/10 text-success",
        cyan: "border-info/20 bg-info/10 text-info",
        destructive:
          "border-destructive/20 bg-destructive/10 text-destructive",

        // ── Filled tier ───────────────────────────────────────────
        // Mirrors `.badge-live` / `.badge-warn` / `.badge-error` /
        // `.badge-ev` in the canonical preview at
        // `~/.gstack/projects/usezombie/designs/design-system-20260508-0831/preview.html`.
        // Filled fills carry the colour as currency — use them for
        // genuine status, not chrome decoration. Dark text comes
        // from the theme-fixed `--on-pulse` token (same dark hex
        // works on every saturated colour).
        live: "border-pulse bg-pulse text-on-pulse",
        warn: "border-warning bg-warning text-on-pulse",
        error: "border-destructive bg-destructive text-on-pulse",
        // Evidence badges anchor a line-numbered log reference —
        // outline form so they read as a citation, not a status.
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
