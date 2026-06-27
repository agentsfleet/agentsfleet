import { Slot } from "@radix-ui/react-slot";
import { cva, type VariantProps } from "class-variance-authority";
import { type ComponentProps } from "react";

import { cn } from "../utils";

const dashboardPanelVariants = cva(
  [
    "rounded-lg border border-border bg-card",
    "transition-colors duration-snap ease-snap",
    "hover:border-border-strong",
  ].join(" "),
  {
    variants: {
      padding: {
        none: "p-0",
        compact: "p-lg",
        default: "p-xl",
      },
    },
    defaultVariants: {
      padding: "default",
    },
  },
);

export type DashboardPanelProps = ComponentProps<"div"> &
  VariantProps<typeof dashboardPanelVariants> & {
    asChild?: boolean;
  };

export function DashboardPanel({
  asChild = false,
  padding,
  className,
  ref,
  ...props
}: DashboardPanelProps) {
  const Comp = asChild ? Slot : "div";
  return (
    <Comp
      ref={ref}
      data-dashboard-panel=""
      className={cn(dashboardPanelVariants({ padding }), className)}
      {...props}
    />
  );
}

export function DashboardPanelHeader({
  className,
  ref,
  ...props
}: ComponentProps<"div">) {
  return (
    <div
      ref={ref}
      className={cn(
        "flex flex-col gap-2 md:flex-row md:items-start md:justify-between",
        className,
      )}
      {...props}
    />
  );
}

export function DashboardPanelTitle({
  className,
  ref,
  ...props
}: ComponentProps<"h2">) {
  return (
    <h2
      ref={ref}
      className={cn("text-heading font-medium leading-heading text-foreground", className)}
      {...props}
    />
  );
}

export function DashboardPanelDescription({
  className,
  ref,
  ...props
}: ComponentProps<"p">) {
  return (
    <p
      ref={ref}
      className={cn("text-body-sm leading-body-sm text-muted-foreground", className)}
      {...props}
    />
  );
}

export function DashboardPanelContent({
  className,
  ref,
  ...props
}: ComponentProps<"div">) {
  return <div ref={ref} className={cn("mt-lg", className)} {...props} />;
}

export function DashboardPanelFooter({
  className,
  ref,
  ...props
}: ComponentProps<"div">) {
  return (
    <div
      ref={ref}
      className={cn("mt-lg border-t border-border pt-lg", className)}
      {...props}
    />
  );
}

export { dashboardPanelVariants };
