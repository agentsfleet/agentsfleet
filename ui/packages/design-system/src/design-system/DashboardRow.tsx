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

export type DashboardRowProps = ComponentProps<"div"> & {
  icon?: ReactNode;
  title: ReactNode;
  description?: ReactNode;
  meta?: ReactNode;
  action?: ReactNode;
};

export function DashboardRow({
  icon,
  title,
  description,
  meta,
  action,
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
        <div className="font-medium text-foreground">{title}</div>
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
