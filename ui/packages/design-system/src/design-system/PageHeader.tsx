import { type ComponentProps, type ReactNode } from "react";
import { cn } from "../utils";

/*
 * PageHeader — standard dashboard page top bar. Two shapes, one primitive:
 *
 *   • bare (back-compat) — children laid out as a flex row (title left, an
 *     optional action passed as a child on the right). Unchanged for existing
 *     call sites.
 *   • structured — pass `description` (rendered directly below the title via
 *     <PageDescription>) and/or `actions` (pinned top-right). The title
 *     (children) + description form a left column; the action aligns to its top.
 *
 * RSC-safe. Pairs with <PageTitle>. PageLayout owns space after the header,
 * so every dashboard page gets the same vertical rhythm.
 */
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
  // Back-compat: no description and no actions → the original bare flex row.
  if (description == null && actions == null) {
    return (
      <div ref={ref} className={cn("flex items-center justify-between", className)} {...props}>
        {children}
      </div>
    );
  }
  return (
    <div ref={ref} className={cn("flex items-start justify-between gap-6", className)} {...props}>
      <div className="min-w-0">
        {children}
        {description != null ? <PageDescription>{description}</PageDescription> : null}
      </div>
      {actions != null ? <div className="flex-none">{actions}</div> : null}
    </div>
  );
}

/** The muted secondary line under a page title. */
export function PageDescription({ className, ...props }: ComponentProps<"p">) {
  return <p className={cn("mt-1.5 text-body-sm text-muted-foreground", className)} {...props} />;
}

export default PageHeader;
