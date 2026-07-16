"use client";

import { useEffect, useState } from "react";
import { Button, WakePulse } from "@agentsfleet/design-system";
import { useFleetEventStream } from "@/components/domain/useFleetEventStream";
import { AGENTSFLEET_STATUS } from "@/lib/api/fleets";
import {
  INSTALL_STEP,
  advanceInstallStep,
  isInstallComplete,
  rankOf,
  type InstallStepId,
} from "@/lib/streaming/install-steps";
import { listFleetsAction } from "../actions";
import { stepLine, type StateLine } from "./install-flow";
import { StateList } from "./install-state-list";

type Props = {
  workspaceId: string;
  fleetId: string;
  fleetName: string;
  onOpen: () => void;
};

const STATUS_RECONCILE_BASE_MS = 500;
const STATUS_RECONCILE_CAP_MS = 5_000;
const STATUS_RECONCILE_ATTEMPTS = 12;

function useInstallStatusReconciliation(
  workspaceId: string,
  fleetId: string,
  installStep: InstallStepId | null,
): InstallStepId | null {
  const [reconciledStep, setReconciledStep] = useState<InstallStepId | null>(null);
  useEffect(() => {
    if (installStep === INSTALL_STEP.READY || installStep === INSTALL_STEP.ERROR) return;
    let cancelled = false;
    let timer: ReturnType<typeof setTimeout> | null = null;
    let attempt = 0;
    async function reconcile() {
      const result = await listFleetsAction(workspaceId, { limit: 100 });
      if (cancelled) return;
      const fleet = result.ok
        ? result.data.items.find((candidate) => candidate.id === fleetId)
        : null;
      if (fleet?.status === AGENTSFLEET_STATUS.ACTIVE) {
        setReconciledStep(INSTALL_STEP.READY);
        return;
      }
      attempt += 1;
      if (attempt >= STATUS_RECONCILE_ATTEMPTS) {
        setReconciledStep(INSTALL_STEP.ERROR);
        return;
      }
      const delay = Math.min(STATUS_RECONCILE_BASE_MS * 2 ** attempt, STATUS_RECONCILE_CAP_MS);
      timer = setTimeout(() => void reconcile(), delay);
    }
    timer = setTimeout(() => void reconcile(), STATUS_RECONCILE_BASE_MS);
    return () => {
      cancelled = true;
      if (timer) clearTimeout(timer);
    };
  }, [fleetId, installStep, workspaceId]);
  return reconciledStep;
}

// The post-create surface. It consumes the existing Server-Sent Events (SSE)
// fleet-event stream via
// useFleetEventStream — each `install:*` frame advances `installStep` with no
// delay, while durable status reconciliation covers a missed ephemeral frame.
// It renders the creating→provisioning→ready ladder. On
// `install:ready` (the fleet has flipped installing→active on the server) it
// surfaces "Open fleet", which lands in the full-height steer/chat.
export function InstallStreamSteps({ workspaceId, fleetId, fleetName, onOpen }: Props) {
  // No server-rendered seed: this fleet was created a tick ago, so the stream
  // starts empty and the install frames drive it. The 201 already told us the
  // fleet is `installing`, so we render `creating` until the first frame lands.
  const { installStep } = useFleetEventStream(workspaceId, fleetId, []);
  const reconciledStep = useInstallStatusReconciliation(workspaceId, fleetId, installStep);
  const current = reconciledStep
    ? advanceInstallStep(installStep, reconciledStep) ?? INSTALL_STEP.CREATING
    : installStep ?? INSTALL_STEP.CREATING;
  const done = isInstallComplete(current);

  return (
    <>
      <StateList lines={ladderLines(current)} />
      {done ? (
        <div className="flex items-center gap-md border-t border-border px-lg py-md">
          <span className="inline-flex items-center gap-2 text-sm text-foreground">
            <WakePulse live className="inline-block h-2 w-2 rounded-full bg-pulse" aria-hidden="true" />
            Installed — <span className="font-mono">{fleetName}</span> is ready
          </span>
          <Button type="button" size="sm" className="ml-auto" onClick={onOpen}>
            Open fleet →
          </Button>
        </div>
      ) : null}
    </>
  );
}

// The fully-walked ladder up to (and including) the current step: completed
// steps render done (✓), the active step renders in-flight. An `error` step
// replaces the ladder tail with the failure line + a manual retry path lives in
// the parent (re-enter), so the spinner never hangs.
function ladderLines(current: InstallStepId): StateLine[] {
  if (current === INSTALL_STEP.ERROR) {
    return [stepLine(INSTALL_STEP.CREATING), stepLine(INSTALL_STEP.ERROR)];
  }
  const ladder: InstallStepId[] = [INSTALL_STEP.CREATING, INSTALL_STEP.PROVISIONING, INSTALL_STEP.READY];
  const currentRank = rankOf(current);
  return ladder
    .filter((step) => rankOf(step) <= currentRank)
    .map((step) => (rankOf(step) < currentRank ? completed(step) : stepLine(step)));
}

// A step the stream has already passed renders as done regardless of its
// in-flight label.
function completed(step: InstallStepId): StateLine {
  const base = stepLine(step);
  return { ...base, tone: "ok", glyph: "✓" };
}
