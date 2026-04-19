import { type HTMLAttributes, type ReactNode } from "react";
import { cn } from "../utils";

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
}

export function EmptyState({
  title,
  description,
  icon,
  action,
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
        "flex flex-col items-center justify-center gap-3 rounded-md border border-dashed border-border",
        "bg-card/50 p-6 sm:p-10 text-center",
        className,
      )}
      {...rest}
    >
      {icon ? (
        <div className="text-muted-foreground" aria-hidden="true">
          {icon}
        </div>
      ) : null}
      <h3 className="text-base font-semibold text-foreground">{title}</h3>
      {description ? (
        <p className="max-w-sm text-sm text-muted-foreground">{description}</p>
      ) : null}
      {action ? <div className="pt-2">{action}</div> : null}
    </div>
  );
}

export default EmptyState;
