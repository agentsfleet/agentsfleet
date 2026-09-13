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
 *
 * A LABELLED section renders <section>, not <div>, without being asked.
 *
 * ARIA gives a plain <div> no role, so `aria-label` on one is dropped and the
 * name never reaches a reader. Two call sites passed a label without `asChild`
 * and lost their names that way, while nine others remembered the wrapper — a
 * component whose accessibility depends on the caller remembering is a
 * component that will keep losing names. The tag now follows the label.
 */
type Props = ComponentProps<"div"> & {
  gap?: boolean;
  asChild?: boolean;
};

export default function Section({ gap, asChild, className, ref, ...rest }: Props) {
  const labelled = rest["aria-label"] != null || rest["aria-labelledby"] != null;
  const Comp = asChild ? Slot : labelled ? "section" : "div";
  return (
    <Comp
      ref={ref}
      data-section={gap ? "gap" : "stack"}
      className={cn(
        "grid min-w-0 grid-cols-1 gap-xl",
        gap && "py-5xl [&+[data-section=gap]]:pt-0",
        className,
      )}
      {...rest}
    />
  );
}
