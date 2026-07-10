import { clsx, type ClassValue } from "clsx";
import { extendTailwindMerge } from "tailwind-merge";

export type { ClassValue };

// The design system's custom font-size scale (theme.css `--text-*`: eyebrow,
// body, body-sm, ...) collides with tailwind-merge's default text-color
// group — both match the `text-{word}` shape, so unconfigured twMerge
// guesses "color" and silently drops whichever of the two comes first (e.g.
// `cn("text-eyebrow", "text-muted-foreground")` → only the color survives).
// Registering the scale under font-size fixes the classification.
const twMerge = extendTailwindMerge({
  extend: {
    classGroups: {
      "font-size": [
        "text-display-xl",
        "text-display-lg",
        "text-display-md",
        "text-heading",
        "text-eyebrow",
        "text-body-lg",
        "text-body",
        "text-body-sm",
        "text-label",
        "text-mono",
        "text-fluid-hero",
        "text-fluid-display-lg",
        "text-fluid-display-md",
      ],
    },
  },
});

export function cn(...inputs: ClassValue[]) {
  return twMerge(clsx(inputs));
}
