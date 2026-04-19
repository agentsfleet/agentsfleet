import * as SeparatorPrimitive from "@radix-ui/react-separator";
import { type ComponentProps } from "react";
import { cn } from "../utils";

/*
 * Separator — Radix Separator.Root wrapper. RSC-safe (Radix Separator
 * has no hooks), React 19 ref-as-prop. Horizontal renders a 1px tall
 * full-width line; vertical renders a 1px wide full-height line.
 */
export type SeparatorProps = ComponentProps<typeof SeparatorPrimitive.Root>;

export function Separator({
  className,
  orientation = "horizontal",
  decorative = true,
  ref,
  ...props
}: SeparatorProps) {
  return (
    <SeparatorPrimitive.Root
      ref={ref}
      decorative={decorative}
      orientation={orientation}
      className={cn(
        "shrink-0 bg-border",
        orientation === "horizontal" ? "h-px w-full" : "h-full w-px",
        className,
      )}
      {...props}
    />
  );
}

export default Separator;
