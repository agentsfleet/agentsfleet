import { type ComponentProps } from "react";
import { cn } from "../utils";

/*
 * Display primitives — marketing-scale typography per DESIGN_SYSTEM.md
 * §Type scale. Tokens:
 *   display-xl  64 / 1.0  / -0.025em — marketing hero h1
 *   display-lg  40 / 1.1  / -0.02em  — section heads on marketing & docs
 *
 * Both use a fluid clamp() between display-md (28) and the target size
 * for responsive marketing layouts. Renders as <h1>/<h2> by default;
 * RSC-safe.
 */

const DISPLAY_XL_CLASS =
  "font-mono text-[clamp(40px,6vw,64px)] leading-[1.05] tracking-[-0.025em] font-medium text-text m-0";
const DISPLAY_LG_CLASS =
  "font-mono text-[clamp(28px,4vw,40px)] leading-[1.15] tracking-[-0.02em] font-medium text-text m-0";

export type DisplayXLProps = ComponentProps<"h1">;
export type DisplayLGProps = ComponentProps<"h2">;

export function DisplayXL({ className, ref, ...props }: DisplayXLProps) {
  return <h1 ref={ref} className={cn(DISPLAY_XL_CLASS, className)} {...props} />;
}

export function DisplayLG({ className, ref, ...props }: DisplayLGProps) {
  return <h2 ref={ref} className={cn(DISPLAY_LG_CLASS, className)} {...props} />;
}
