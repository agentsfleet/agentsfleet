import { type ComponentProps } from "react";
import { cn } from "../utils";

/*
 * PageTitle — standard dashboard h1 styling. Renders as <h1> by default;
 * pages that render inside a larger shell can pass as="div" or their own
 * heading element via the `as` prop (light composition, no Radix Slot
 * since title is a leaf). RSC-safe.
 */
type PageTitleProps = ComponentProps<"h1">;

export function PageTitle({ className, ref, ...props }: PageTitleProps) {
  return (
    <h1
      ref={ref}
      className={cn("text-xl font-semibold tracking-tight", className)}
      {...props}
    />
  );
}

export type { PageTitleProps };
export default PageTitle;
