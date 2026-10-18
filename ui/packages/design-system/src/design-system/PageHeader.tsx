import { type ComponentProps, type ReactNode } from "react";
import { cn } from "../utils";

/** Page title and optional description, with actions aligned alongside on wider screens. */
export type PageHeaderProps = ComponentProps<"div"> & {
  /** Secondary line rendered directly below the title (muted, body-sm). */
  description?: ReactNode;
  /** Action cluster pinned to the top-right of the header. */
  actions?: ReactNode;
};

export function PageHeader({
  className,
  description,
  actions,
  children,
  ref,
  ...props
}: PageHeaderProps) {
  return (
    <div ref={ref} className={cn("flex flex-col items-start justify-between gap-4 border-b border-border pb-2xl sm:flex-row sm:gap-6", className)} {...props}>
      <div className="min-w-0 max-w-full">
        {children}
        {description != null ? <PageDescription>{description}</PageDescription> : null}
      </div>
      {actions != null ? <div className="flex max-w-full flex-none flex-wrap gap-2">{actions}</div> : null}
    </div>
  );
}

/** The muted secondary line under a page title. */
export function PageDescription({ className, ...props }: ComponentProps<"p">) {
  return <p className={cn("mt-md max-w-prose text-body text-muted-foreground", className)} {...props} />;
}

export default PageHeader;
