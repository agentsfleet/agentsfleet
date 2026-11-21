import type { HTMLAttributes, ReactNode } from "react";
import { cn } from "../utils";

type ResourceRowProps = HTMLAttributes<HTMLElement> & {
  as?: "li" | "div";
  icon: ReactNode;
  actions: ReactNode;
  density?: "default" | "compact";
};

/** A settings resource keeps its identity and actions together as the available width changes. */
export function ResourceRow({as: Row = "li", icon, children, actions, density, className, ...props}: ResourceRowProps) {
  return (
    <Row {...props} className={cn("flex min-w-0 flex-wrap items-center gap-xl p-xl summary:px-5",
      density === "compact" ? "min-h-21" : "summary:py-5", className)}>
      <span aria-hidden="true" className="hidden size-10 shrink-0 items-center justify-center rounded-control border border-border summary:flex">
        {icon}
      </span>
      <div className="min-w-0 basis-full summary:flex-1 summary:basis-auto">{children}</div>
      {actions}
    </Row>
  );
}
