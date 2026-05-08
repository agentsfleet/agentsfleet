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
        "flex h-10 w-full rounded-md border border-border bg-secondary",
        "px-lg py-md text-body-sm text-foreground font-mono",
        "placeholder:text-muted-foreground",
        "focus:outline-none focus:ring-2 focus:ring-ring focus:border-border-strong",
        "disabled:cursor-not-allowed disabled:opacity-50",
        "transition-colors ease-snap",
        className,
      )}
      {...props}
    />
  );
}

export default Input;
