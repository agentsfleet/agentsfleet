import { type ComponentProps, type ReactNode } from "react";
import { cn } from "../utils";
import { SectionLabel } from "./SectionLabel";

export type SectionHeaderProps = ComponentProps<"div"> & {
  actions?: ReactNode;
  /**
   * Forwarded to `SectionLabel`. `"h2"` when the eyebrow is the section's only
   * name; `"p"` when a `PageTitle` above already names it.
   *
   * This defaulted to `SectionLabel`'s own `"h2"` and had no way through, so
   * every page using `SectionHeader` shipped a 12px `<h2>` under its 28px
   * `<h1>` and could not opt out. `SectionLabel` had carried `as` for exactly
   * this since it was written; only the forwarding was missing.
   */
  as?: "h2" | "p";
};

export function SectionHeader({
  actions,
  children,
  className,
  ref,
  as = "h2",
  ...props
}: SectionHeaderProps) {
  return (
    <div
      ref={ref}
      className={cn(
        // items-center, not baseline: the label is one 16px line sharing a row with
        // a 40px action, and baseline alignment left 13px of slack under the
        // label — the gap to the section body measured 29px, off the 4px grid.
        // Centering puts the label on the action's optical middle and lands the
        // gap on 28.
        "flex min-w-0 flex-wrap items-center justify-between gap-md",
        className,
      )}
      {...props}
    >
      <SectionLabel as={as}>{children}</SectionLabel>
      {actions != null ? <div className="flex-none">{actions}</div> : null}
    </div>
  );
}

export default SectionHeader;
