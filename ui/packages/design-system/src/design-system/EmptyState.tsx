import { type HTMLAttributes, type ReactNode } from "react";
import { cn } from "../utils";

export const EMPTY_STATE_SURFACE = { panel: "panel", plain: "plain" } as const;
export type EmptyStateSurface = typeof EMPTY_STATE_SURFACE[keyof typeof EMPTY_STATE_SURFACE];

/*
 * EmptyState — neutral surface for zero-data screens. RSC-safe. Callers
 * provide title/description plus optional icon + action slots. Uses
 * role=status + aria-live=polite so screen readers announce transitions
 * from loading → empty.
 */
export interface EmptyStateProps extends HTMLAttributes<HTMLDivElement> {
  title: string;
  description?: string;
  icon?: ReactNode;
  action?: ReactNode;
  surface?: EmptyStateSurface;
}

export function EmptyState({
  title,
  description,
  icon,
  action,
  surface = EMPTY_STATE_SURFACE.panel,
  className,
  ...rest
}: EmptyStateProps) {
  return (
    <div
      data-slot="empty-state"
      data-testid="empty-state"
      role="status"
      aria-live="polite"
      className={cn(
        "flex flex-col",
        surface === EMPTY_STATE_SURFACE.plain
          ? "items-start gap-sm bg-transparent py-xl text-left"
          : "items-center justify-center gap-3 rounded-md border border-dashed border-border bg-card/50 p-6 sm:p-10 text-center",
        className,
      )}
      {...rest}
    >
      {icon ? (
        <div className="text-muted-foreground" aria-hidden="true">
          {icon}
        </div>
      ) : null}
      <h2 className={surface === EMPTY_STATE_SURFACE.plain
        ? "text-body-sm font-medium text-muted-foreground"
        : "text-heading leading-heading font-semibold text-foreground"}>{title}</h2>
      {description ? (
        <p className={cn("text-body-sm text-muted-foreground", surface === EMPTY_STATE_SURFACE.panel && "max-w-sm")}>{description}</p>
      ) : null}
      {action ? <div className="pt-2">{action}</div> : null}
    </div>
  );
}

export default EmptyState;
