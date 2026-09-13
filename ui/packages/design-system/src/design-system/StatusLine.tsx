import { type ComponentProps } from "react";
import { cn } from "../utils";

/*
 * StatusLine — one dense line of figures, the way a terminal's status line
 * reads: monospace, tabular digits, a hairline between cells, no card. Each
 * cell is a StatusLineItem that puts an icon or a visually hidden label ahead
 * of its figure, so a screen reader hears "Tokens 3,255" where a sighted
 * reader sees a glyph. Owns typography, colour and the rules between cells;
 * the caller places the line. RSC-safe.
 *
 * One row, always: the rules between cells are `divide-x`, which draws on
 * every cell but the last, so a wrapped second row would open with an inset
 * and the first row would end on an orphaned rule. Narrower than its cells,
 * the line scrolls sideways instead — the same answer the section strip
 * gives, and the one a terminal gives.
 */

export type StatusLineTone = "neutral" | "foreground" | "pulse" | "success" | "warning" | "danger";

export type StatusLineProps = ComponentProps<"div"> & { "aria-label": string };

export type StatusLineItemProps = ComponentProps<"span"> & { tone?: StatusLineTone };

const toneClass: Record<StatusLineTone, string> = {
  neutral: "text-muted-foreground",
  foreground: "text-foreground",
  pulse: "text-pulse",
  success: "text-success",
  warning: "text-warning",
  danger: "text-destructive",
};

export function StatusLine({ className, ref, ...props }: StatusLineProps) {
  // A generic element does not reliably expose an accessible name; a group
  // does, and a group is what the cells are — one line of related figures.
  // Not a region: a status line is not a landmark.
  return (
    <div
      ref={ref}
      role="group"
      className={cn(
        "flex min-w-0 flex-nowrap items-center overflow-x-auto divide-x divide-border",
        "font-mono text-label leading-label tabular-nums text-muted-foreground",
        className,
      )}
      {...props}
    />
  );
}

export function StatusLineItem({ tone = "neutral", className, ref, ...props }: StatusLineItemProps) {
  return (
    <span
      ref={ref}
      data-tone={tone}
      className={cn(
        "inline-flex shrink-0 items-center gap-sm whitespace-nowrap px-md first:pl-0 last:pr-0",
        toneClass[tone],
        className,
      )}
      {...props}
    />
  );
}
