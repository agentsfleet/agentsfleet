import { type ComponentProps, type ReactNode } from "react";
import { cn } from "../utils";
import { Badge } from "./Badge";
import { SectionLabel } from "./SectionLabel";

export type SectionHeaderProps = ComponentProps<"div"> & {
  actions?: ReactNode;
  count?: number;
  description?: ReactNode;
  /** Heading level when the section is nested beneath a page title. */
  as?: "h2" | "p";
};

export function SectionHeader({
  actions,
  count,
  description,
  children,
  className,
  ref,
  as = "h2",
  ...props
}: SectionHeaderProps) {
  return (
    <div
      ref={ref}
      className={cn(
        "flex min-w-0 flex-wrap items-center justify-between gap-md max-summary:gap-xl",
        className,
      )}
      {...props}
    >
      <div className="min-w-0">
        <SectionLabel as={as} variant="navigation" className={cn("mb-0 font-semibold text-foreground", count !== undefined && "flex items-center gap-md")}>
          {children}
          {count !== undefined ? <Badge asChild variant="count"><span>{count}</span></Badge> : null}
        </SectionLabel>
        {description ? <p className="mt-sm text-label leading-label text-muted-foreground">{description}</p> : null}
      </div>
      {actions != null ? <div className="flex-none">{actions}</div> : null}
    </div>
  );
}
