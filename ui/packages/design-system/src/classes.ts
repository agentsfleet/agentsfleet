import { cn } from "./utils";

export type UiButtonVariant = "primary" | "ghost" | "double-border";

export function uiButtonClass(variant: UiButtonVariant = "primary") {
  const base = "z-btn";
  if (variant === "ghost") return `${base} z-btn--ghost`;
  if (variant === "double-border") return `${base} z-btn--double`;
  return base;
}

export function uiCardClass(featured?: boolean, className?: string) {
  return cn("z-card", featured && "z-card--featured", className);
}
