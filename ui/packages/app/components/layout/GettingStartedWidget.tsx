"use client";

import { useState, useTransition } from "react";
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
  putPreferenceAction,
} from "@/lib/actions/preferences";
import { captureProductEvent } from "@/lib/analytics/posthog";
import { EVENTS } from "@/lib/analytics/events";
import {
  useOnboardingProgress,
  type OnboardingPollingMode,
} from "./use-onboarding-progress";

type Props = {
  workspaceId: string;
  pollingMode?: OnboardingPollingMode;
};

// Bottom-left onboarding widget — the checklist's only home once a fleet exists
// (the single-route refactor removed the page). Mirrors the same tick rail via
// the one derivation, so it can't disagree with the empty-state checklist. Its
// dismiss/collapse state is a server preference (§5), so it survives a device
// change; the browser has no token, so it pulls its progress through a server
// action. Successful local actions invalidate that progress, while focus and a
// visible-tab timer cover changes that happen elsewhere. Fail-open: while
// loading, or before the first successful read, the widget stays hidden rather
// than flashing a zeroed checklist.
export default function GettingStartedWidget({ workspaceId, pollingMode = "mounted" }: Props) {
  const [localPreference, setLocalPreference] = useState<{
    workspaceId: string;
    collapsed: boolean;
    dismissed: boolean;
  } | null>(null);
  const local = localPreference?.workspaceId === workspaceId ? localPreference : null;
  const progress = useOnboardingProgress(
    workspaceId,
    local?.dismissed ?? false,
    pollingMode,
  );
  const [, startTransition] = useTransition();

  if (!progress) return null;

  const collapsed = local?.collapsed ?? progress.collapsed;
  // A confirmed server dismissal wins over a stale local `false` left behind
  // by an earlier collapse toggle. Local `true` still provides optimistic hide.
  const dismissed = progress.dismissed || local?.dismissed === true;
  if (dismissed) return null;

  const steps = deriveSteps(progress.inputs);
  const completed = completedRequiredCount(progress.inputs);
  const complete = isOnboardingComplete(progress.inputs);

  function persist(key: (typeof PREFERENCE_KEY)[keyof typeof PREFERENCE_KEY], value: boolean) {
    startTransition(() => {
      void putPreferenceAction(workspaceId, key, value);
    });
  }

  function toggleCollapse() {
    const next = !collapsed;
    setLocalPreference({ workspaceId, collapsed: next, dismissed });
    persist(PREFERENCE_KEY.COLLAPSED, next);
  }

  function dismiss() {
    // Optimistic hide — dismiss is only offered when onboarding is complete, so
    // a lost write costs at most the widget reappearing on the next load, never
    // hidden onboarding.
    setLocalPreference({ workspaceId, collapsed, dismissed: true });
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
