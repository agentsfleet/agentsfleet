"use client";

import { useEffect, useState, useTransition } from "react";
import { ChevronDownIcon, ChevronUpIcon, XIcon } from "lucide-react";
import { cn, EYEBROW_CLASS, IconAction } from "@agentsfleet/design-system";
import OnboardingRail from "@/components/domain/OnboardingRail";
import {
  completedRequiredCount,
  deriveSteps,
  isOnboardingComplete,
  REQUIRED_STEP_COUNT,
} from "@/lib/onboarding";
import { PREFERENCE_KEY } from "@/lib/api/preferences";
import {
  getOnboardingSnapshotAction,
  putPreferenceAction,
  type OnboardingSnapshot,
} from "@/lib/actions/preferences";
import { captureProductEvent } from "@/lib/analytics/posthog";
import { EVENTS } from "@/lib/analytics/events";

type Props = { workspaceId: string };

// Bottom-left onboarding widget — the checklist's only home once a fleet exists
// (the single-route refactor removed the page). Mirrors the same tick rail via
// the one derivation, so it can't disagree with the empty-state checklist. Its
// dismiss/collapse state is a server preference (§5), so it survives a device
// change; the browser has no token, so it pulls its snapshot through a server
// action. Fail-open: while loading, or on error, the widget stays hidden rather
// than flashing a zeroed checklist, and a genuine read failure returns an
// undismissed snapshot so onboarding is never hidden by a failure.
export default function GettingStartedWidget({ workspaceId }: Props) {
  const [snapshot, setSnapshot] = useState<OnboardingSnapshot | null>(null);
  const [collapsed, setCollapsed] = useState(false);
  const [dismissed, setDismissed] = useState(false);
  const [, startTransition] = useTransition();

  useEffect(() => {
    let live = true;
    setSnapshot(null);
    getOnboardingSnapshotAction(workspaceId)
      .then((res) => {
        if (!live) return;
        if (res.ok) {
          setSnapshot(res.data);
          setCollapsed(res.data.collapsed);
          setDismissed(res.data.dismissed);
        }
      })
      // Fail-open: a rejected snapshot read leaves the widget hidden (null
      // snapshot) rather than crashing the shell — the checklist reappears on
      // the next successful load, and a read failure never marks onboarding
      // dismissed.
      .catch(() => {});
    return () => {
      live = false;
    };
  }, [workspaceId]);

  if (!snapshot || dismissed) return null;

  const steps = deriveSteps(snapshot.inputs);
  const completed = completedRequiredCount(snapshot.inputs);
  const complete = isOnboardingComplete(snapshot.inputs);

  function persist(key: (typeof PREFERENCE_KEY)[keyof typeof PREFERENCE_KEY], value: boolean) {
    startTransition(() => {
      void putPreferenceAction(workspaceId, key, value);
    });
  }

  function toggleCollapse() {
    const next = !collapsed;
    setCollapsed(next);
    persist(PREFERENCE_KEY.COLLAPSED, next);
  }

  function dismiss() {
    // Optimistic hide — dismiss is only offered when onboarding is complete, so
    // a lost write costs at most the widget reappearing on the next load, never
    // hidden onboarding.
    setDismissed(true);
    persist(PREFERENCE_KEY.DISMISSED, true);
    captureProductEvent(EVENTS.onboarding_dismissed, {
      workspace_id: workspaceId,
      completed_steps: completed,
    });
  }

  return (
    <div className="mx-2 mb-2 rounded-md border border-border bg-card p-2">
      <div className="flex items-center justify-between">
        <span className={cn(EYEBROW_CLASS, "text-muted-foreground")}>Getting started</span>
        <div className="flex items-center gap-1">
          <span className="font-mono text-label text-pulse tabular-nums">
            {completed}/{REQUIRED_STEP_COUNT}
          </span>
          <IconAction
            label={collapsed ? "Expand getting started" : "Collapse getting started"}
            onClick={toggleCollapse}
          >
            {collapsed ? <ChevronUpIcon size={14} /> : <ChevronDownIcon size={14} />}
          </IconAction>
          {complete ? (
            <IconAction label="Dismiss getting started" onClick={dismiss}>
              <XIcon size={14} />
            </IconAction>
          ) : null}
        </div>
      </div>

      {collapsed ? null : (
        <div className="mt-2">
          <OnboardingRail workspaceId={workspaceId} steps={steps} compact />
        </div>
      )}
    </div>
  );
}
