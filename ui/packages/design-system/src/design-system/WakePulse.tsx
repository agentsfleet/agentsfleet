import { Slot } from "@radix-ui/react-slot";
import type { ComponentProps } from "react";

import { cn } from "../utils";

export interface WakePulseProps extends ComponentProps<"span"> {
  /**
   * The pulse only fires on actually-live entities. The boolean is
   * required (not defaulted) so consumers cannot accidentally pulse
   * non-live UI: the type system forces an explicit decision at every
   * call site.
   */
  live: boolean;
  /** Compose onto the child element (Radix Slot pattern, like Button). */
  asChild?: boolean;
}

/*
 * The signature wake-pulse — the only animation in the design system.
 *
 * Sets `data-live` on the rendered element when `live` is true; the
 * `[data-live]` rule in tokens.css drives the keyframe (2.4s ease-in-out
 * infinite, expanding glow ring in --pulse-glow). When `live` flips to
 * false the attribute is removed and the animation stops the same frame.
 *
 * `prefers-reduced-motion: reduce` is honoured entirely in CSS — tokens.css
 * replaces the keyframe with a static glow ring at low opacity. Nothing
 * for the component to do.
 */
export function WakePulse({
  live,
  asChild = false,
  className,
  children,
  ...rest
}: WakePulseProps) {
  const Comp = asChild ? Slot : "span";
  return (
    <Comp
      data-live={live ? true : undefined}
      className={cn(className)}
      {...rest}
    >
      {children}
    </Comp>
  );
}
