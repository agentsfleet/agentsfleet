import { type ComponentProps, type ReactNode } from "react";

import { cn } from "../utils";

export type DashboardRowGroupProps = ComponentProps<"div">;

export function DashboardRowGroup({
  className,
  ref,
  ...props
}: DashboardRowGroupProps) {
  return (
    <div
      ref={ref}
      className={cn("overflow-hidden rounded-lg border border-border bg-card", className)}
      {...props}
    />
  );
}

// Omit the native `title` (tooltip) attribute before intersecting — otherwise
// TypeScript merges it with the custom `title: ReactNode` prop below into an
// unusable `string & ReactNode` type that only accepts a plain string,
// rejecting any JSX title (e.g. a label + status badge composed together).
export type DashboardRowProps = Omit<ComponentProps<"div">, "title"> & {
  icon?: ReactNode;
  title: ReactNode;
  description?: ReactNode;
  meta?: ReactNode;
  action?: ReactNode;
  /**
   * The element the title renders as. Defaults to `"h3"` so a collection of
   * rows is navigable.
   *
   * It used to be a bare `<div>`, which left routes built from these rows —
   * integrations, runners — with a page title, a section label, and then an
   * unstructured pile: nothing to jump between and no count to announce. The
   * title already reads as a heading; only its rank was missing.
   *
   * `"div"` stays available for a row whose title is not the name of anything
   * a reader would navigate to.
   */
  titleAs?: "h3" | "h4" | "div";
};

export function DashboardRow({
  icon,
  title,
  description,
  meta,
  action,
  titleAs: TitleTag = "h3",
  className,
  ref,
  ...props
}: DashboardRowProps) {
  return (
    <div
      ref={ref}
      className={cn(
        "flex items-start gap-3 border-b border-border px-lg py-md last:border-b-0",
        "transition-colors duration-snap ease-snap hover:bg-secondary",
        className,
      )}
      {...props}
    >
      {icon ? (
        <span
          className="grid h-8 w-8 flex-none place-items-center rounded-md border border-border bg-secondary text-muted-foreground"
          aria-hidden="true"
        >
          {icon}
        </span>
      ) : null}
      <div className="min-w-0 flex-1">
        <TitleTag className="font-medium text-foreground">{title}</TitleTag>
        {description ? (
          <div className="mt-1 text-body-sm leading-body-sm text-muted-foreground">
            {description}
          </div>
        ) : null}
        {meta ? <div className="mt-2">{meta}</div> : null}
      </div>
      {action ? <div className="ml-auto flex-none">{action}</div> : null}
    </div>
  );
}
