import { Slot } from "@radix-ui/react-slot";
import { cva, type VariantProps } from "class-variance-authority";
import { type ComponentProps } from "react";

import { cn } from "../utils";

/*
 * Operational Restraint button. Mono chrome, no rounded-full, no
 * decorative gradients, no shadow on the resting state. Seven variants:
 *   default       — pulse fill, near-black text — the live CTA
 *   destructive   — error fill — terminal actions only
 *   outline       — transparent, border-strong outline — quiet chrome
 *   secondary     — surface-2, subtle
 *   ghost         — transparent, muted text — recedes into the page
 *   link          — text-only with underline-on-hover
 *   double-border — heavy 2px border in --pulse — emphasis-on-outline
 *                   for secondary CTAs that should still draw the eye
 *                   without committing to a fill (e.g. "Setup your
 *                   personal dashboard"). No inset/outer shadows; the
 *                   weight comes from the border alone (spec: borders
 *                   preferred over shadows).
 *
 * `asChild` composes any router <Link> through Radix Slot.
 *
 *   <Button>Wake</Button>                              // default (pulse)
 *   <Button variant="ghost" size="sm">x</Button>
 *   <Button variant="double-border">Setup dashboard</Button>
 *   <Button asChild><a href="/x">Go</a></Button>
 */
export const buttonVariants = cva(
  [
    "inline-flex items-center justify-center gap-2 whitespace-nowrap",
    "rounded-md border font-mono font-medium",
    "transition-colors duration-snap ease-snap",
    "focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 focus-visible:ring-offset-background",
    "disabled:cursor-not-allowed disabled:opacity-50",
    "[&_svg]:pointer-events-none [&_svg]:size-4 [&_svg]:shrink-0",
  ].join(" "),
  {
    variants: {
      variant: {
        // Primary CTA. Per `docs/DESIGN_SYSTEM.md` + the canonical preview
        // at `~/.gstack/projects/usezombie/designs/design-system-20260508-0831/preview.html`,
        // the primary button is mint background + theme-fixed dark text
        // (`--on-pulse`). The text token does NOT swap with theme — the
        // mint background is the same in dark + light, so the foreground
        // must also be the same. The `[&_a]` selectors force the colour
        // through `asChild` wrappers since the website's
        // `a { color: inherit }` reset would otherwise win the cascade.
        default:
          "border-pulse bg-pulse text-on-pulse hover:bg-pulse-dim hover:border-pulse-dim [&_a]:text-on-pulse [&_a]:no-underline [&_a:hover]:text-on-pulse",
        // Destructive — saturated red fill. Same theme-fixed dark text
        // story as the primary variant: `--destructive-foreground`
        // resolves to `var(--bg)` which swaps to parchment in light
        // mode → invisible white-ish text on red. `text-on-pulse`
        // (theme-fixed `#0a0d0e`) holds dark in both modes.
        destructive:
          "border-transparent bg-destructive text-on-pulse hover:opacity-90 [&_a]:text-on-pulse [&_a]:no-underline",
        outline:
          "border-border-strong bg-transparent text-foreground hover:bg-muted",
        // Secondary CTA. Mirrors `.btn` in the canonical preview:
        // surface-2 background, --text foreground, border-strong.
        // Hover surfaces to surface-3 + text-subtle border (the
        // chrome breathes one tier when interactive).
        secondary:
          "border-border-strong bg-secondary text-foreground hover:bg-accent hover:border-text-subtle [&_a]:text-foreground [&_a]:no-underline",
        // Ghost — transparent chrome, muted text. Matches `.btn-ghost`
        // in the preview: hover swaps to --surface-1 + foreground text.
        // Note `bg-card` resolves to --surface-1 (Layer 1 alias) — the
        // production theme exposes `--card: var(--surface-1)`. Using
        // `bg-card` over `bg-muted` (--surface-2) lifts ghost hover one
        // tier shallower per the preview reference.
        ghost:
          "border-transparent bg-transparent text-muted-foreground hover:text-foreground hover:bg-card [&_a]:text-muted-foreground [&_a]:no-underline [&_a:hover]:text-foreground",
        link:
          "border-transparent bg-transparent text-pulse underline-offset-4 hover:underline min-h-0 p-0 h-auto",
        // Double-border — heavy mint border on transparent fill. Hover
        // fills mint and demotes the text to the theme-fixed dark token
        // (same swap-safe story as `default` and `destructive`).
        "double-border":
          "border-2 border-primary bg-transparent text-foreground hover:bg-primary hover:text-on-pulse [&_a]:text-foreground [&_a]:no-underline [&_a:hover]:text-on-pulse",
      },
      size: {
        default: "h-10 px-xl py-lg text-body-sm",
        sm: "h-8 px-lg py-md text-label",
        lg: "h-12 px-2xl py-xl text-body",
        icon: "h-9 w-9 p-0",
      },
    },
    defaultVariants: {
      variant: "default",
      size: "default",
    },
  },
);

export type ButtonVariant = NonNullable<VariantProps<typeof buttonVariants>["variant"]>;
export type ButtonSize = NonNullable<VariantProps<typeof buttonVariants>["size"]>;

export type ButtonProps = ComponentProps<"button"> &
  VariantProps<typeof buttonVariants> & {
    asChild?: boolean;
  };

export function Button({
  className,
  variant,
  size,
  asChild = false,
  type,
  ref,
  ...props
}: ButtonProps) {
  const Comp = asChild ? Slot : "button";
  return (
    <Comp
      ref={ref}
      className={cn(buttonVariants({ variant, size }), className)}
      type={asChild ? undefined : (type ?? "button")}
      {...props}
    />
  );
}

export default Button;

/** Class-string helper for non-React / non-Tailwind-JSX callers. */
export function buttonClassName(
  variant: ButtonVariant = "default",
  size: ButtonSize = "default",
): string {
  return buttonVariants({ variant, size });
}
