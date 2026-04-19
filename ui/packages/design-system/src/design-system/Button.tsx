import { Slot } from "@radix-ui/react-slot";
import { cva, type VariantProps } from "class-variance-authority";
import { type ComponentProps } from "react";
import { cn } from "../utils";

/*
 * Framework-agnostic, RSC-safe Button.
 *
 * Zero router imports. Composition with any link element (Next <Link>,
 * react-router-dom <Link>, plain <a>, custom) goes through Radix Slot
 * via `asChild`. React 19 accepts `ref` as a prop, so no forwardRef.
 *
 * Variants match the shadcn/ui convention: default / destructive /
 * outline / secondary / ghost / link + UseZombie's signature
 * double-border. Sizes: default / sm / lg / icon.
 *
 *   <Button>Click</Button>                                        // <button>, default variant
 *   <Button variant="destructive" size="sm">Delete</Button>
 *   <Button asChild><a href="/x">Go</a></Button>                  // <a> with button styles
 *   <Button asChild><Link to="/x">Go</Link></Button>              // any router <Link>
 */
export const buttonVariants = cva(
  [
    "inline-flex items-center justify-center gap-2 whitespace-nowrap",
    "rounded-full border font-sans font-semibold cursor-pointer",
    "transition-[box-shadow,transform,background-color,border-color,color] ease-fast",
    "hover:-translate-y-px",
    "focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 focus-visible:ring-offset-background",
    "disabled:cursor-not-allowed disabled:opacity-50 disabled:hover:translate-y-0",
    "[&_svg]:pointer-events-none [&_svg]:size-4 [&_svg]:shrink-0",
  ].join(" "),
  {
    variants: {
      variant: {
        default:
          "border-transparent bg-[linear-gradient(120deg,var(--primary),var(--primary-bright))] text-primary-foreground font-bold hover:shadow-cta",
        destructive:
          "border-transparent bg-destructive text-destructive-foreground hover:opacity-90",
        outline:
          "border-border bg-transparent text-foreground hover:bg-muted",
        secondary:
          "border-transparent bg-secondary text-secondary-foreground hover:bg-accent",
        ghost:
          "bg-transparent border-border text-foreground hover:border-primary hover:shadow-subtle",
        link:
          "border-transparent bg-transparent text-info underline-offset-4 hover:underline min-h-0 p-0 h-auto",
        "double-border": [
          "bg-transparent border-2 border-primary text-foreground font-bold",
          "shadow-[inset_0_0_0_2px_var(--background),0_0_0_1px_var(--primary)]",
          "hover:shadow-[inset_0_0_0_2px_var(--background),0_0_16px_var(--primary-glow-strong)]",
        ].join(" "),
      },
      size: {
        default: "min-h-11 px-[1.4rem] py-[0.7rem] text-base",
        sm: "h-8 px-4 text-xs",
        lg: "h-12 px-8 text-base",
        icon: "h-10 w-10 p-0",
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
