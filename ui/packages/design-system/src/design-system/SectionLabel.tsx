import { type ComponentProps } from "react";
import { cn } from "../utils";
import { EYEBROW_CLASS } from "./eyebrow";

const SECTION_LABEL_STYLE = {
  eyebrow: EYEBROW_CLASS,
  navigation: "font-sans text-label leading-label font-medium",
} as const;

/*
 * SectionLabel — eyebrow text above a section (e.g. "Pipeline", "Recent runs",
 * "Artifacts"). Mono, uppercase, muted. React Server Component-safe. Shares
 * EYEBROW_CLASS with every other eyebrow (table headers, nav groups, card
 * micro-labels) so the whole family stays one size. Navigation labels use a
 * quieter, sentence-case variant while retaining the same heading semantics.
 *
 * `as` decides whether the eyebrow IS the section's heading or merely labels
 * one, and the two callers genuinely differ. A dashboard section names itself
 * with this and nothing else, so it stays an <h2> and stays discoverable —
 * that is the default and why it is the default. A marketing section puts a
 * display heading directly beneath it, and there the <h2> is a lie twice over:
 * it announces two headings of equal rank where there is one section, and it
 * gives a 12px label the same standing as the 40px line under it. Those pass
 * `as="p"`.
 */
export type SectionLabelProps = ComponentProps<"h2"> & {
  /** `"h2"` when the eyebrow names the section; `"p"` when a heading follows it. */
  as?: "h2" | "p";
  variant?: keyof typeof SECTION_LABEL_STYLE;
};

export function SectionLabel({ className, ref, as: Tag = "h2", variant = "eyebrow", ...props }: SectionLabelProps) {
  return (
    <Tag
      ref={ref}
      className={cn("mb-3 text-muted-foreground", SECTION_LABEL_STYLE[variant], className)}
      {...props}
    />
  );
}

export default SectionLabel;
