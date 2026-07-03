import { type ComponentProps } from "react";
import { cn } from "../utils";
import { EYEBROW_CLASS } from "./eyebrow";

/*
 * SectionLabel — eyebrow text above a dashboard section (e.g. "Pipeline",
 * "Recent runs", "Artifacts"). Mono, uppercase, muted. React Server
 * Component-safe. Renders as <h2> so dashboard sections stay discoverable.
 * Shares EYEBROW_CLASS with every other eyebrow (table headers, nav groups,
 * card micro-labels) so the whole family stays one size.
 */
export type SectionLabelProps = ComponentProps<"h2">;

export function SectionLabel({ className, ref, ...props }: SectionLabelProps) {
  return (
    <h2
      ref={ref}
      className={cn("mb-3 text-muted-foreground", EYEBROW_CLASS, className)}
      {...props}
    />
  );
}

export default SectionLabel;
