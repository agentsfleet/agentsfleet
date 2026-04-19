import { Slot } from "@radix-ui/react-slot";
import { type ComponentProps } from "react";
import { cn } from "../utils";

/*
 * Section — vertical flow wrapper.
 *
 *   default      : stacked grid, gap-xl between children
 *   gap=true     : adds top+bottom page-section padding;
 *                  consecutive gap sections collapse the top padding
 *                  via [data-section=gap]+[data-section=gap] variant
 *   asChild=true : render as whatever element the child provides
 *                  (<main>, <article>, <section>, etc.)
 */
type Props = ComponentProps<"div"> & {
  gap?: boolean;
  asChild?: boolean;
};

export default function Section({ gap, asChild, className, ref, ...rest }: Props) {
  const Comp = asChild ? Slot : "div";
  return (
    <Comp
      ref={ref}
      data-section={gap ? "gap" : "stack"}
      className={cn(
        "grid gap-xl",
        gap && "py-5xl [&+[data-section=gap]]:pt-0",
        className,
      )}
      {...rest}
    />
  );
}
