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
    "transition-colors ease-snap",
    "focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 focus-visible:ring-offset-background",
    "disabled:cursor-not-allowed disabled:opacity-50",
    "[&_svg]:pointer-events-none [&_svg]:size-4 [&_svg]:shrink-0",
  ].join(" "),
  {
    variants: {
      variant: {
        default:
          "border-transparent bg-primary text-primary-foreground hover:bg-pulse-dim",
        destructive:
          "border-transparent bg-destructive text-destructive-foreground hover:opacity-90",
        outline:
          "border-border-strong bg-transparent text-foreground hover:bg-muted",
        secondary:
          "border-transparent bg-secondary text-secondary-foreground hover:bg-accent",
        ghost:
          "border-transparent bg-transparent text-muted-foreground hover:text-foreground hover:bg-muted",
        link:
          "border-transparent bg-transparent text-pulse underline-offset-4 hover:underline min-h-0 p-0 h-auto",
        "double-border":
          "border-2 border-primary bg-transparent text-foreground hover:bg-primary hover:text-primary-foreground",
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
