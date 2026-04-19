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
        "flex h-10 w-full rounded-lg border border-input bg-muted",
        "px-3 py-2 text-sm text-foreground font-sans",
        "placeholder:text-muted-foreground",
        "focus:outline-none focus:ring-1 focus:ring-ring focus:border-primary",
        "disabled:cursor-not-allowed disabled:opacity-50",
        "transition-[border-color,box-shadow] duration-150",
        className,
      )}
      {...props}
    />
  );
}

export default Input;
