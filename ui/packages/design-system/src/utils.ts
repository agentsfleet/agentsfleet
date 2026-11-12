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
    theme: {
      radius: ["control"],
      leading: ["display-xl", "display-lg", "display-md", "heading", "conversation-heading", "new-fleet-title", "usage-total", "dialog-description", "eyebrow", "body-lg", "body", "body-sm", "label", "mono", "prose", "conversation", "composer"],
      spacing: ["xs", "sm", "md", "lg", "xl", "2xl", "3xl", "4xl", "5xl", "6xl", "chat-input", "chat-inset", "table-inline", "table-block"],
    },
    classGroups: {
      "font-weight": ["font-title", "font-control"],
      "font-size": [
        "text-display-xl",
        "text-display-lg",
        "text-display-md",
        "text-heading",
        "text-conversation-heading",
        "text-description",
        "text-new-fleet-title",
        "text-new-fleet-description",
        "text-usage-total",
        "text-eyebrow",
        "text-body-lg",
        "text-body",
        "text-body-sm",
        "text-label",
        "text-mono",
        "text-conversation",
        "text-composer",
        "text-chat-title",
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
