import { type ComponentProps } from "react";
import { cn } from "../utils";

/*
 * Input — RSC-safe text input. React 19 ref-as-prop. Layer 2 semantic
 * utilities only; placeholder uses muted-foreground to match shadcn
 * convention. A client-only wrapper that ties `disabled` to React 19's
 * `useFormStatus()` can land alongside the first form consumer.
 */
export type InputProps = ComponentProps<"input">;

export function Input({ className, type, ref, ...props }: InputProps) {
  return (
    <input
      ref={ref}
      type={type}
      className={cn(
        "flex min-h-11 w-full rounded-control border border-border bg-background pointer-coarse:min-h-11",
        "px-lg py-sm text-body-sm text-foreground font-sans",
        "placeholder:text-text-dim placeholder:opacity-100",
        "aria-invalid:border-destructive",
        "focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-4 focus-visible:ring-offset-[var(--control-focus-surface)]",
        "disabled:cursor-not-allowed disabled:opacity-50",
        "transition-colors duration-snap ease-snap",
        className,
      )}
      {...props}
    />
  );
}

export default Input;
