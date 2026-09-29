import type { ReactNode } from "react";
import { CircleXIcon } from "lucide-react";
import { cn } from "@agentsfleet/design-system";

const FAILED_OUTCOME_CLASS = "flex min-h-6 items-start gap-xs text-label font-medium leading-label text-foreground";

/**
 * A failure line: the words keep the foreground ink, and the mark carries the
 * failure. Without the mark, a failed reply reads as a small reply rather
 * than as a notice about one. The reply row and the integration tick both use
 * this, so a failure looks the same in either row.
 */
export function FleetFailedOutcome({ children, className }: { children: ReactNode; className?: string }) {
  return (
    <p className={cn(FAILED_OUTCOME_CLASS, className)} data-failed-outcome="true">
      <span className="flex size-4 shrink-0 items-center justify-center">
        <CircleXIcon size={12} className="text-destructive" aria-hidden="true" />
      </span>
      <span>{children}</span>
    </p>
  );
}
