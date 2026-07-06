"use client";

import { Button, WakePulse } from "@agentsfleet/design-system";
import { useFleetEventStream } from "@/components/domain/useFleetEventStream";
import {
  INSTALL_STEP,
  isInstallComplete,
  rankOf,
  type InstallStepId,
} from "@/lib/streaming/install-steps";
import { stepLine, type StateLine } from "./install-flow";
import { StateList } from "./install-state-list";

type Props = {
  workspaceId: string;
  fleetId: string;
  fleetName: string;
  onOpen: () => void;
};

// The post-create surface. It consumes the existing SSE fleet-event stream via
// useFleetEventStream — each `install:*` frame advances `installStep` with no
// polling — and renders the creating→provisioning→ready ladder. On
// `install:ready` (the fleet has flipped installing→active on the server) it
// surfaces "Open fleet", which lands in the full-height steer/chat.
export function InstallStreamSteps({ workspaceId, fleetId, fleetName, onOpen }: Props) {
  // No server-rendered seed: this fleet was created a tick ago, so the stream
  // starts empty and the install frames drive it. The 201 already told us the
  // fleet is `installing`, so we render `creating` until the first frame lands.
  const { installStep } = useFleetEventStream(workspaceId, fleetId, []);
  const current = installStep ?? INSTALL_STEP.CREATING;
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
