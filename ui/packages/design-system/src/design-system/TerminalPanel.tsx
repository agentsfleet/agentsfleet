import { type ComponentProps, type ReactNode } from "react";

import { cn } from "../utils";

export type TerminalPanelProps = ComponentProps<"div"> & {
  title: ReactNode;
  tag?: ReactNode;
  bodyClassName?: string;
};

export function TerminalPanel({
  title,
  tag,
  bodyClassName,
  className,
  children,
  ref,
  ...props
}: TerminalPanelProps) {
  return (
    <div
      ref={ref}
      data-terminal-panel=""
      className={cn("overflow-hidden rounded-lg border border-border bg-card", className)}
      {...props}
    >
      <div className="flex items-center gap-md border-b border-border bg-surface-deep px-lg py-md">
        <span className="flex gap-1.5" aria-hidden="true">
          <span className="h-2.5 w-2.5 rounded-full bg-border-strong" />
          <span className="h-2.5 w-2.5 rounded-full bg-border-strong" />
          <span className="h-2.5 w-2.5 rounded-full bg-border-strong" />
        </span>
        <span className="font-mono text-label text-muted-foreground">{title}</span>
        {tag ? (
          <span className="ml-auto font-mono text-label uppercase tracking-label text-text-subtle">
            {tag}
          </span>
        ) : null}
      </div>
      <div className={bodyClassName}>{children}</div>
    </div>
  );
}
