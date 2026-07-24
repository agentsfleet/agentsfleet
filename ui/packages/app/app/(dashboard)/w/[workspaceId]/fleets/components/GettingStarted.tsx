"use client";

import { useEffect, useState, useTransition } from "react";
import { useRouter } from "next/navigation";
import { Alert, Button, PageHeader, PageLayout, PageTitle } from "@agentsfleet/design-system";
import OnboardingRail from "@/components/domain/OnboardingRail";
import {
  completedRequiredCount,
  deriveSteps,
  type OnboardingInputs,
} from "@/lib/onboarding";
import { PREFERENCE_KEY } from "@/lib/api/preferences";
import { putPreferenceAction } from "@/lib/actions/preferences";
import { captureProductEvent } from "@/lib/analytics/posthog";
import { EVENTS } from "@/lib/analytics/events";
import { requestOnboardingRefresh } from "@/lib/onboarding-refresh";

type Props = { workspaceId: string; inputs: OnboardingInputs };

// The Getting Started checklist — the Wall's empty state (there is no separate
// route; the Wall IS the entry point). Renders the shared tick rail from the
// one derivation, fires the viewed funnel event once, and owns the only
// interactive step: the manual CLI tick, which the server cannot detect.
export default function GettingStarted({ workspaceId, inputs }: Props) {
  const router = useRouter();
  const [cliTicked, setCliTicked] = useState(inputs.cliTicked);
  const [error, setError] = useState<string | null>(null);
  const [pending, startTransition] = useTransition();

  const effectiveInputs: OnboardingInputs = { ...inputs, cliTicked };
  const steps = deriveSteps(effectiveInputs);
  const completed = completedRequiredCount(effectiveInputs);

  useEffect(() => {
    captureProductEvent(EVENTS.onboarding_viewed, {
      workspace_id: workspaceId,
      completed_steps: completedRequiredCount(inputs),
    });
    // Fire once per mount for this workspace; `inputs` is server-derived and
    // stable across the mount.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [workspaceId]);

  function tickCli() {
    setError(null);
    // Optimistic: the CLI step is the user's own claim, and a failed write must
    // not silently swallow it — so we flip locally, then reconcile on failure.
    setCliTicked(true);
    startTransition(async () => {
      const result = await putPreferenceAction(
        workspaceId,
        PREFERENCE_KEY.CLI_TICKED,
        true,
      );
      if (!result.ok) {
        setCliTicked(false);
        setError("Couldn't save that just now. Try again.");
        return;
      }
      captureProductEvent(EVENTS.onboarding_cli_ticked, {
        workspace_id: workspaceId,
      });
      requestOnboardingRefresh(workspaceId);
      router.refresh();
    });
  }

  return (
    <PageLayout className="max-w-measure">
      <PageHeader description="Install a fleet, connect its credential, watch it run.">
        <PageTitle>Getting started</PageTitle>
      </PageHeader>

      <div>
        <div className="text-body-sm text-muted-foreground" aria-live="polite">
          {completed}/{steps.filter((s) => s.required).length} done
        </div>

        <div className="mt-3">
          <OnboardingRail workspaceId={workspaceId} steps={steps} />
        </div>

        {!cliTicked ? (
          <div className="mt-4">
            <Button variant="ghost" size="sm" onClick={tickCli} disabled={pending}>
              {pending ? "Saving…" : "I've installed the CLI"}
            </Button>
          </div>
        ) : null}

        {error ? (
          <Alert variant="destructive" className="mt-3">{error}</Alert>
        ) : null}
      </div>
    </PageLayout>
  );
}
