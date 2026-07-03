import { type ComponentProps } from "react";
import { cn } from "../utils";

/*
 * SectionLabel — eyebrow text above a dashboard section (e.g. "Pipeline",
 * "Recent runs", "Artifacts"). Mono, uppercase, muted. React Server
 * Component-safe. Renders as <h2> so dashboard sections stay discoverable.
 */
export type SectionLabelProps = ComponentProps<"h2">;

export function SectionLabel({ className, ref, ...props }: SectionLabelProps) {
  return (
    <h2
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
