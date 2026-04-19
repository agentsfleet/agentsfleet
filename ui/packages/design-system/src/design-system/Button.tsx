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
 *   <Button variant="primary">Click</Button>             // <button>
 *   <Button asChild><a href="/x">Go</a></Button>         // <a> with button styles
 *   <Button asChild><Link to="/x">Go</Link></Button>     // any router <Link>
 */
export const buttonVariants = cva(
  [
    "inline-flex items-center justify-center gap-2 whitespace-nowrap",
    "min-h-11 rounded-full border px-[1.4rem] py-[0.7rem]",
    "font-sans text-base font-bold cursor-pointer",
    "transition-[box-shadow,transform] ease-fast",
    "hover:-translate-y-px",
    "focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 focus-visible:ring-offset-background",
    "disabled:cursor-not-allowed disabled:opacity-50 disabled:hover:translate-y-0",
  ].join(" "),
  {
    variants: {
      variant: {
        primary:
          "border-transparent bg-[linear-gradient(120deg,var(--primary),var(--primary-bright))] text-primary-foreground hover:shadow-cta",
        ghost:
          "bg-transparent border-border text-foreground hover:border-primary hover:shadow-subtle",
        "double-border": [
          "bg-transparent border-2 border-primary text-foreground",
          "shadow-[inset_0_0_0_2px_var(--background),0_0_0_1px_var(--primary)]",
          "hover:shadow-[inset_0_0_0_2px_var(--background),0_0_16px_var(--primary-glow-strong)]",
        ].join(" "),
      },
    },
    defaultVariants: { variant: "primary" },
  },
);

export type ButtonVariant = NonNullable<VariantProps<typeof buttonVariants>["variant"]>;

export type ButtonProps = ComponentProps<"button"> &
  VariantProps<typeof buttonVariants> & {
    asChild?: boolean;
  };

export function Button({
  className,
  variant,
  asChild = false,
  type,
  ref,
  ...props
}: ButtonProps) {
  const Comp = asChild ? Slot : "button";
  return (
    <Comp
      ref={ref}
      className={cn(buttonVariants({ variant }), className)}
      type={asChild ? undefined : (type ?? "button")}
      {...props}
    />
  );
}

export default Button;

/** Class-string helper for non-React / non-Tailwind-JSX callers. */
export function buttonClassName(variant: ButtonVariant = "primary"): string {
  return buttonVariants({ variant });
}
