import { cn } from "./utils";

/*
 * Legacy class-string helpers kept for the few non-Button primitives
 * still using BEM-style class names. New components use Tailwind
 * utilities via cva (see Button.tsx for the pattern). These shrink
 * component-by-component as M26.3 rewrites land.
 */
export function uiCardClass(featured?: boolean, className?: string) {
  return cn("z-card", featured && "z-card--featured", className);
}

/* Button variant type + helper re-exported from Button.tsx via the
 * barrel; keep these name aliases for callers that imported them
 * from "@usezombie/design-system/classes" before the rewrite. */
export type { ButtonVariant as UiButtonVariant } from "./design-system/Button";
export { buttonClassName as uiButtonClass } from "./design-system/Button";
