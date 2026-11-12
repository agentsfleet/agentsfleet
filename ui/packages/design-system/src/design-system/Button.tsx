import { Slot } from "@radix-ui/react-slot";
import { cva, type VariantProps } from "class-variance-authority";
import { type ComponentProps } from "react";

import { cn } from "../utils";

export const buttonVariants = cva(
  [
    "inline-flex items-center justify-center gap-2 whitespace-nowrap",
    "rounded-control border font-sans font-control",
    "pointer-coarse:min-h-11 pointer-coarse:min-w-11",
    "control-motion transition-colors duration-snap ease-snap",
    "focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-4 focus-visible:ring-offset-background",
    "disabled:cursor-not-allowed disabled:bg-muted disabled:text-text-dim disabled:border-border-subtle disabled:opacity-100",
    "[&_svg]:pointer-events-none [&_svg]:size-4 [&_svg]:shrink-0",
  ].join(" "),
  {
    variants: {
      variant: {
        default:
          "border-0 bg-cta text-cta-foreground disabled:ring-1 disabled:ring-inset disabled:ring-border-subtle enabled:hover:brightness-[var(--control-hover)] [&_a]:text-cta-foreground [&_a]:no-underline [&_a:hover]:text-cta-foreground",
        destructive:
          "border-transparent bg-destructive text-on-pulse hover:opacity-90 [&_a]:text-on-pulse [&_a]:no-underline",
        outline:
          "border-border bg-card text-foreground hover:bg-muted",
        secondary:
          "border-border bg-card text-foreground hover:bg-muted [&_a]:text-foreground [&_a]:no-underline",
        ghost:
          "border-transparent bg-transparent text-muted-foreground hover:text-foreground hover:bg-muted [&_a]:text-muted-foreground [&_a]:no-underline [&_a:hover]:text-primary",
        link:
          "border-transparent bg-transparent text-pulse underline-offset-4 hover:underline min-h-0 p-0 h-auto",
        "double-border":
          "border-2 border-primary bg-transparent font-semibold text-primary hover:bg-primary/10 hover:text-primary [&_a]:text-primary [&_a]:no-underline [&_a:hover]:text-primary",
      },
      size: {
        default: "min-h-10 px-xl py-sm text-label leading-label",
        sm: "min-h-10 px-xl py-sm text-label leading-label",
        lg: "h-11 px-xl py-sm text-body",
        card: "h-auto min-h-16 p-lg text-body-sm",
        icon: "h-10 w-10 shrink-0 p-0",
        "icon-compact": "h-8 w-8 shrink-0 p-0",
        "icon-sm": "h-8 w-8 shrink-0 p-0",
        // A control that sits on the eyebrow line — a collapsible nav group's
        // label, say. Geometry only; the caller adds EYEBROW_CLASS for the
        // type, exactly as a plain eyebrow does, so the two never drift. A
        // size rather than className overrides because `cn` registers only
        // the font-size tokens with tailwind-merge: `px-2` against `sm`'s
        // `px-lg` is decided by stylesheet order, and it lost.
        eyebrow: "h-4 border-0 px-2 py-0 font-normal",
      },
      wrap: { true: "h-auto max-w-full whitespace-normal", false: "" },
    },
    defaultVariants: {
      variant: "default",
      size: "default",
      wrap: false,
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
  wrap,
  type,
  ref,
  ...props
}: ButtonProps) {
  const Comp = asChild ? Slot : "button";
  return (
    <Comp
      ref={ref}
      className={cn(buttonVariants({ variant, size, wrap }), className)}
      type={asChild ? undefined : (type ?? "button")}
      {...props}
    />
  );
}

export default Button;

export function buttonClassName(
  variant: ButtonVariant = "default",
  size: ButtonSize = "default",
): string {
  return buttonVariants({ variant, size });
}
