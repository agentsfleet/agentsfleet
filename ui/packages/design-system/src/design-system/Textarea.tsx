import { type ComponentProps } from "react";
import { cn } from "../utils";

/*
 * Textarea — RSC-safe multi-line text input. React 19 ref-as-prop. Mirrors
 * Input.tsx semantic utilities so a Textarea sitting next to an Input in a
 * form reads as the same surface. Default min-height of 80px keeps the
 * collapsed state usable; consumers override via `rows` or className.
 */
export type TextareaProps = ComponentProps<"textarea">;

export function Textarea({ className, ref, ...props }: TextareaProps) {
  return (
    <textarea
      ref={ref}
      className={cn(
        "flex min-h-20 w-full rounded-md border border-border bg-secondary",
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

export default Textarea;
