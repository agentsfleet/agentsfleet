import { type ComponentProps } from "react";
import { cn } from "../utils";

/*
 * Refpill — a compact, rounded mono chip that names what *references* a vault
 * secret or resource (canonical preview `.refpill`). Distinct from the squared
 * status Badge / StatusPill: this reads as a reference token, not a status.
 * RSC-safe, React 19 ref-as-prop. Colours come from Layer-2 semantic tokens so
 * a theme-level change propagates automatically.
 */
export type RefpillProps = ComponentProps<"span">;

export function Refpill({ className, ref, ...props }: RefpillProps) {
  return (
    <span
      ref={ref}
      className={cn(
        "inline-flex items-center rounded-full border border-border bg-secondary",
        "px-2 py-0.5 font-mono text-label leading-none text-muted-foreground",
        className,
      )}
      {...props}
    />
  );
}

export default Refpill;
