import type { ComponentProps } from "react";

import { cn } from "../utils";

/** The ten frames, in order; tokens.css steps a column of them one line at a time. */
export const BRAILLE_FRAMES = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"] as const;
/** Shown in place of the column when motion is reduced. */
export const BRAILLE_REST_FRAME = "⠶";

export type BrailleSpinnerProps = Omit<ComponentProps<"span">, "children">;

/*
 * A working glyph beside a label that says what is happening ("Thinking", a
 * waiting verb). Decorative, so assistive tech skips it: the label carries the
 * meaning. Use Spinner when the glyph IS the status — it is a live region, and
 * a live region cannot sit inside a button.
 *
 * CSS-only: the `[data-braille-*]` rules in tokens.css step the column with
 * `transform`, so the compositor animates it and no script runs per frame.
 * Reduced motion hides the column and shows the rest frame.
 */
export function BrailleSpinner({ className, ...rest }: BrailleSpinnerProps) {
  return (
    <span aria-hidden="true" data-braille-spinner="" className={cn(className)} {...rest}>
      <span data-braille-frames="">
        {BRAILLE_FRAMES.map((frame) => (
          <span key={frame}>{frame}</span>
        ))}
      </span>
      <span data-braille-rest="">{BRAILLE_REST_FRAME}</span>
    </span>
  );
}
