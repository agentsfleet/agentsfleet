import { type ComponentProps } from "react";
import { cn } from "../utils";

/*
 * Display primitives — marketing-scale typography per DESIGN_SYSTEM.md
 * §Type scale. Composed entirely from design-system token utilities:
 *
 *   DisplayXL  text-fluid-hero        (clamp 40 ↔ 64) — marketing hero <h1>
 *   DisplayLG  text-fluid-display-lg  (clamp 28 ↔ 40) — section heads
 *
 * Both forward Layer-0 fluid tokens declared in tokens.css §Fluid display
 * sizes via theme.css @theme inline. No arbitrary clamp() / leading /
 * tracking values — closing the UI Component Substitution Gate's
 * "marketing-display typography exception" carve-out.
 *
 * Line-height note: DisplayXL uses leading-display-xl (--lh-display-xl: 1.0).
 * The pre-token version had leading-[1.05] which was off-spec — DESIGN_SYSTEM.md
 * §Typography pins display-xl line-height at 1.0. The two-line marketing hero
 * was visually verified at 1.0 before this change shipped; descenders clear.
 *
 * Renders as <h1>/<h2> by default; RSC-safe.
 */

const DISPLAY_XL_CLASS =
  "font-mono text-fluid-hero leading-display-xl tracking-display-xl font-medium text-text m-0";
const DISPLAY_LG_CLASS =
  "font-mono text-fluid-display-lg leading-display-md tracking-display-lg font-medium text-text m-0";

export type DisplayXLProps = ComponentProps<"h1">;
export type DisplayLGProps = ComponentProps<"h2">;

export function DisplayXL({ className, ref, ...props }: DisplayXLProps) {
  return <h1 ref={ref} className={cn(DISPLAY_XL_CLASS, className)} {...props} />;
}

export function DisplayLG({ className, ref, ...props }: DisplayLGProps) {
  return <h2 ref={ref} className={cn(DISPLAY_LG_CLASS, className)} {...props} />;
}
