import { cva, type VariantProps } from "class-variance-authority";
import { type ComponentProps } from "react";
import { cn } from "../utils";

/*
 * DescriptionList — semantic <dl>/<dt>/<dd> primitive for label/value
 * pairs. Two layouts:
 *   - inline (default): each <dt>+<dd> sits on the same row, label on
 *     the left, value right-aligned. Wraps gracefully on narrow widths.
 *   - stacked: term above detail; for vertical forms.
 *
 * `DescriptionDetails` accepts `mono?: boolean` to render the value in
 * font-mono / text-xs (IDs, hashes, credential refs) — matches the
 * existing dashboard convention.
 *
 * Inline layout renders one <div class="flex …"> per row group inside
 * the <dl>. Callers can either pass <DescriptionTerm>/<DescriptionDetails>
 * pairs directly (one row each) or wrap each pair in their own group
 * element when rendering many rows.
 */

/*
 * Inline layout requires each <DescriptionTerm>/<DescriptionDetails>
 * pair to be wrapped in a <div> (one row group per <div>) — the
 * `[&>div]:flex …` utilities only target direct <div> children. Bare
 * <dt>/<dd> children inside <dl layout="inline"> render with no flex
 * row layout. Stacked layout has no such requirement.
 */
export const dlVariants = cva("text-sm", {
  variants: {
    layout: {
      inline:
        "space-y-3 [&>div]:flex [&>div]:flex-wrap [&>div]:items-baseline [&>div]:justify-between [&>div]:gap-2",
      stacked: "space-y-3",
    },
  },
  defaultVariants: { layout: "inline" },
});

export interface DescriptionListProps
  extends ComponentProps<"dl">,
    VariantProps<typeof dlVariants> {
  /**
   * Layout shape.
   *
   * - `inline` (default): each row is a `<div>` wrapping a `<DescriptionTerm>`
   *   + `<DescriptionDetails>` pair (label left, value right). The `<div>`
   *   wrapper is required — flex utilities target direct `<div>` children
   *   only; bare `<dt>`/`<dd>` siblings will not pick up the row layout.
   * - `stacked`: term above detail, no per-row wrapper required.
   */
  layout?: "inline" | "stacked";
}

export function DescriptionList({
  layout = "inline",
  className,
  ref,
  ...props
}: DescriptionListProps) {
  return (
    <dl ref={ref} className={cn(dlVariants({ layout }), className)} {...props} />
  );
}

export function DescriptionTerm({
  className,
  ref,
  ...props
}: ComponentProps<"dt">) {
  return (
    <dt
      ref={ref}
      className={cn("text-muted-foreground", className)}
      {...props}
    />
  );
}

export interface DescriptionDetailsProps extends ComponentProps<"dd"> {
  mono?: boolean;
}

export function DescriptionDetails({
  mono = false,
  className,
  ref,
  ...props
}: DescriptionDetailsProps) {
  return (
    <dd
      ref={ref}
      className={cn(mono ? "font-mono text-xs" : "", className)}
      {...props}
    />
  );
}

export default DescriptionList;
