import { type ComponentProps } from "react";
import { cn } from "../utils";

/*
 * SectionLabel — eyebrow text above a dashboard section (e.g. "Pipeline",
 * "Recent runs", "Artifacts"). Mono, uppercase, muted. RSC-safe. Renders
 * as <p> by default.
 */
export type SectionLabelProps = ComponentProps<"p">;

export function SectionLabel({ className, ref, ...props }: SectionLabelProps) {
  return (
    <p
      ref={ref}
      className={cn(
        "mb-3 font-mono text-xs uppercase tracking-widest text-muted-foreground",
        className,
      )}
      {...props}
    />
  );
}

export default SectionLabel;
