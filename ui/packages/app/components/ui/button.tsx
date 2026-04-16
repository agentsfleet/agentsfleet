import * as React from "react";
import { Slot } from "@radix-ui/react-slot";
import { cva, type VariantProps } from "class-variance-authority";
import { cn } from "@usezombie/design-system/utils";

const buttonVariants = cva(
  "inline-flex items-center justify-center gap-2 whitespace-nowrap rounded-full text-sm font-semibold transition-all motion-reduce:transition-none disabled:pointer-events-none disabled:opacity-50 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 focus-visible:ring-offset-background [&_svg]:pointer-events-none [&_svg]:size-4 [&_svg]:shrink-0",
  {
    variants: {
      variant: {
        default:
          "bg-gradient-to-r from-primary to-primary-bright text-primary-foreground shadow-[0_0_0_0_var(--primary-glow)] hover:shadow-[0_0_20px_var(--primary-glow-strong)] hover:-translate-y-px",
        ghost:
          "border border-border bg-transparent text-muted-foreground hover:border-primary hover:text-foreground hover:shadow-[0_0_12px_var(--primary-glow)]",
        destructive:
          "bg-destructive text-destructive-foreground hover:opacity-90",
        outline:
          "border border-border bg-transparent text-foreground hover:bg-muted",
        secondary:
          "bg-secondary text-secondary-foreground hover:bg-accent",
        link:
          "text-info underline-offset-4 hover:underline p-0 h-auto",
        "double-border":
          "border-2 border-primary bg-transparent text-foreground shadow-[inset_0_0_0_2px_var(--background)] hover:shadow-[inset_0_0_0_2px_var(--background),0_0_16px_var(--primary-glow-strong)]",
      },
      size: {
        default: "h-10 px-5 py-2",
        sm:      "h-8 rounded-full px-4 text-xs",
        lg:      "h-12 rounded-full px-8 text-base",
        icon:    "h-10 w-10",
      },
    },
    defaultVariants: {
      variant: "default",
      size: "default",
    },
  },
);

export interface ButtonProps
  extends React.ButtonHTMLAttributes<HTMLButtonElement>,
    VariantProps<typeof buttonVariants> {
  asChild?: boolean;
}

const Button = React.forwardRef<HTMLButtonElement, ButtonProps>(
  ({ className, variant, size, asChild = false, ...props }, ref) => {
    const Comp = asChild ? Slot : "button";
    return (
      <Comp
        data-slot="button"
        data-variant={variant ?? "default"}
        data-size={size ?? "default"}
        className={cn(buttonVariants({ variant, size, className }))}
        ref={ref}
        {...props}
      />
    );
  },
);
Button.displayName = "Button";

export { Button, buttonVariants };
