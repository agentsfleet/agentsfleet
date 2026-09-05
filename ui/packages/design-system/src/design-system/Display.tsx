import { type ComponentProps } from "react";
import { cn } from "../utils";

// Website display typography. Font roles and fluid sizes come from shared tokens.

const DISPLAY_XL_CLASS =
  "font-display text-fluid-hero leading-display-xl tracking-display-xl font-medium text-text m-0";
const DISPLAY_LG_CLASS =
  "font-display text-fluid-display-lg leading-display-md tracking-display-lg font-medium text-text m-0";

export type DisplayXLProps = ComponentProps<"h1">;
export type DisplayLGProps = ComponentProps<"h2">;

export function DisplayXL({ className, ref, ...props }: DisplayXLProps) {
  return <h1 ref={ref} className={cn(DISPLAY_XL_CLASS, className)} {...props} />;
}

export function DisplayLG({ className, ref, ...props }: DisplayLGProps) {
  return <h2 ref={ref} className={cn(DISPLAY_LG_CLASS, className)} {...props} />;
}
