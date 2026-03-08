import * as React from "react";
import { cn } from "@/lib/utils";

const Input = React.forwardRef<HTMLInputElement, React.InputHTMLAttributes<HTMLInputElement>>(
  ({ className, type, ...props }, ref) => (
    <input
      type={type}
      className={cn(
        "flex h-10 w-full rounded-lg border border-[var(--z-border)] bg-[var(--z-surface-1)]",
        "px-3 py-2 text-sm text-[var(--z-text-primary)] font-[var(--z-font-sans)]",
        "placeholder:text-[var(--z-text-dim)]",
        "focus:outline-none focus:ring-1 focus:ring-[var(--z-orange)] focus:border-[var(--z-orange)]",
        "disabled:cursor-not-allowed disabled:opacity-50",
        "transition-[border-color,box-shadow] duration-150",
        className,
      )}
      ref={ref}
      {...props}
    />
  ),
);
Input.displayName = "Input";

export { Input };
