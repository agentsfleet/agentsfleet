import { Slot } from "@radix-ui/react-slot";
import { cva, type VariantProps } from "class-variance-authority";
import { type ComponentProps } from "react";

import { cn } from "../utils";

export const buttonVariants = cva(
  [
    "inline-flex items-center justify-center gap-2 whitespace-nowrap",
    "rounded-md border font-sans font-medium",
    "pointer-coarse:min-h-11 pointer-coarse:min-w-11",
    "transition-colors duration-snap ease-snap",
    "focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 focus-visible:ring-offset-background",
    "disabled:cursor-not-allowed disabled:opacity-50",
    "[&_svg]:pointer-events-none [&_svg]:size-4 [&_svg]:shrink-0",
  ].join(" "),
  {
    variants: {
      variant: {
        default:
          "border-cta bg-cta text-cta-foreground hover:bg-cta-hover hover:border-cta-hover [&_a]:text-cta-foreground [&_a]:no-underline [&_a:hover]:text-cta-foreground",
        destructive:
          "border-transparent bg-destructive text-on-pulse hover:opacity-90 [&_a]:text-on-pulse [&_a]:no-underline",
        outline:
          "border-border-strong bg-transparent text-foreground hover:bg-muted",
        secondary:
          "border-border-strong bg-secondary text-foreground hover:bg-accent hover:border-text-subtle [&_a]:text-foreground [&_a]:no-underline",
        ghost:
          "border-transparent bg-transparent text-muted-foreground hover:text-foreground hover:bg-card [&_a]:text-muted-foreground [&_a]:no-underline [&_a:hover]:text-foreground",
        link:
          "border-transparent bg-transparent text-pulse underline-offset-4 hover:underline min-h-0 p-0 h-auto",
        "double-border":
          "border-2 border-primary bg-transparent font-semibold text-primary hover:bg-primary/10 hover:text-primary [&_a]:text-primary [&_a]:no-underline [&_a:hover]:text-primary",
      },
      size: {
        default: "h-10 px-xl py-lg text-body-sm",
        sm: "h-8 px-lg py-md text-body-sm",
        lg: "h-12 px-2xl py-xl text-body",
        icon: "h-9 w-9 p-0",
        "icon-sm": "h-6 w-6 shrink-0 p-0",
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
