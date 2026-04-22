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
        "flex min-h-20 w-full rounded-lg border border-input bg-muted",
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

export default Textarea;
