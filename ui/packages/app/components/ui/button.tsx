import * as React from "react";
import { Slot } from "@radix-ui/react-slot";
import { cva, type VariantProps } from "class-variance-authority";
import { cn } from "@usezombie/design-system/utils";

const buttonVariants = cva(
  "inline-flex items-center justify-center gap-2 whitespace-nowrap rounded-full text-sm font-semibold transition-all disabled:pointer-events-none disabled:opacity-50 [&_svg]:pointer-events-none [&_svg]:size-4 [&_svg]:shrink-0",
  {
    variants: {
      variant: {
        default:
          "bg-gradient-to-r from-[var(--z-orange)] to-[var(--z-orange-bright)] text-[#111] shadow-[0_0_0_0_var(--z-glow-orange)] hover:shadow-[0_0_20px_var(--z-glow-strong)] hover:-translate-y-px",
        ghost:
          "border border-[var(--z-border)] bg-transparent text-[var(--z-text-muted)] hover:border-[var(--z-orange)] hover:text-[var(--z-text-primary)] hover:shadow-[0_0_12px_var(--z-glow-orange)]",
        destructive:
          "bg-[var(--z-red)] text-[var(--z-text-primary)] hover:opacity-90",
        outline:
          "border border-[var(--z-border)] bg-transparent hover:bg-[var(--z-surface-1)] text-[var(--z-text-primary)]",
        secondary:
          "bg-[var(--z-surface-1)] text-[var(--z-text-primary)] hover:bg-[var(--z-surface-2)]",
        link:
          "text-[var(--z-cyan)] underline-offset-4 hover:underline p-0 h-auto",
        "double-border":
          "border-2 border-[var(--z-orange)] bg-transparent text-[var(--z-text-primary)] shadow-[inset_0_0_0_2px_var(--z-bg-0)] hover:shadow-[inset_0_0_0_2px_var(--z-bg-0),0_0_16px_var(--z-glow-strong)]",
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
        className={cn(buttonVariants({ variant, size, className }))}
        ref={ref}
        {...props}
      />
    );
  },
);
Button.displayName = "Button";

export { Button, buttonVariants };
