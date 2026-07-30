"use client";

import { useEffect } from "react";
import { Button, EmptyState, PageHeader, PageLayout, PageTitle } from "@agentsfleet/design-system";
import { AlertTriangleIcon } from "lucide-react";
import { EVENTS } from "@/lib/analytics/events";
import { captureProductEvent } from "@/lib/analytics/posthog";
import { RetryCountdownRing } from "./RetryCountdownRing";
import { useErrorRetry } from "./use-error-retry";

// Dashboard error boundary. A transient failure loading a dashboard surface
// (e.g. the workspace list on the entry redirect) renders an honest retry state
// rather than a misleading empty/create-first screen or a blank page. `reset`
// re-renders the segment, re-running the failed server work.
//
// Two things it does beyond rendering that state:
//
//   1. It RETRIES on its own. The copy calls the failure transient, so waiting
//      for a click before acting on that is a claim the product declines to
//      back. `useErrorRetry` owns the ladder and the backoff.
//   2. It REPORTS. The boundary used to take `error` and drop it, so every
//      dashboard failure was invisible — no log, no count, no way to know a
//      route was broken for someone. The support line below promises a human
//      is on it, and that promise is only as true as this capture.

const ERROR_TITLE = "Couldn't load this page";
const ERROR_DESCRIPTION_RETRYING = "This looks transient — retrying automatically.";
const ERROR_DESCRIPTION_EXHAUSTED =
  "This did not clear on its own. Retry, or come back in a few minutes.";
const SUPPORT_NOTE = "A support agent is working on this.";
const RETRY_NOW_LABEL = "Retry now";
const RETRY_LABEL = "Retry";

export default function DashboardError({
  error,
  reset,
}: {
  error: Error & { digest?: string };
  reset: () => void;
}) {
  const retry = useErrorRetry(reset);
  const { attempt, maxAttempts, exhausted, secondsRemaining } = retry;

  useEffect(() => {
    // One capture per failure, not per countdown tick. `attempt`/`maxAttempts`
    // are fixed for the life of a mount (the ladder position is read once, in a
    // useState initializer), and a remounted boundary carries a NEW Error
    // object — so this fires exactly once per real failure.
    captureProductEvent(EVENTS.dashboard_error_shown, {
      error_name: error.name,
      digest: error.digest,
      // Reported 0-based so a first failure reads as 0, and a value equal to
      // the budget means the automatic attempts were spent without recovering.
      attempt: attempt === null ? maxAttempts : attempt - 1,
    });
  }, [error, attempt, maxAttempts]);

  return (
    <PageLayout>
      <PageHeader>
        <PageTitle>Something went wrong</PageTitle>
      </PageHeader>
      <EmptyState
        icon={<AlertTriangleIcon size={32} />}
        title={ERROR_TITLE}
        description={exhausted ? ERROR_DESCRIPTION_EXHAUSTED : ERROR_DESCRIPTION_RETRYING}
        action={
          <div className="flex flex-col items-center gap-md">
            {exhausted ? null : (
              <RetryCountdownRing progress={retry.progress} label={`${secondsRemaining}s`} />
            )}
            {/* The countdown is announced here, once, rather than from the ring:
                `polite` so it never interrupts, and the attempt count tells a
                non-sighted user the page is working through a budget. */}
            <p aria-live="polite" className="font-mono text-label text-muted-foreground">
              {exhausted
                ? `Stopped after ${maxAttempts} automatic attempts.`
                : `Retrying in ${secondsRemaining}s · attempt ${attempt} of ${maxAttempts}`}
            </p>
            <Button type="button" onClick={retry.retryNow} data-testid="dashboard-error-retry">
              {exhausted ? RETRY_LABEL : RETRY_NOW_LABEL}
            </Button>
            <p className="font-mono text-label text-muted-foreground">{SUPPORT_NOTE}</p>
          </div>
        }
      />
    </PageLayout>
  );
}
