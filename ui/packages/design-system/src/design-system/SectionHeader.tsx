import { type ComponentProps, type ReactNode } from "react";
import { cn } from "../utils";
import { SectionLabel } from "./SectionLabel";

export type SectionHeaderProps = ComponentProps<"div"> & {
  actions?: ReactNode;
};

export function SectionHeader({
  actions,
  children,
  className,
  ref,
  ...props
}: SectionHeaderProps) {
  return (
    <div
      ref={ref}
      className={cn(
        "flex min-w-0 flex-wrap items-baseline justify-between gap-md",
        className,
      )}
      {...props}
    >
      <SectionLabel>{children}</SectionLabel>
      {actions != null ? <div className="flex-none">{actions}</div> : null}
    </div>
  );
}

export default SectionHeader;
