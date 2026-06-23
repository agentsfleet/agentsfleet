// Pure install-progression model shared by the SSE registry, the InstallStates
// component, and their tests. Nothing here touches EventSource or React — it
// maps the backend's synthetic `install:*` frames onto an ordered, renderable
// step sequence and resolves which step the fleet is currently on.
//
// The terminal `ready` step is the cross-tier flip signal: when it lands the
// fleet's status has gone installing→active on the server, so the UI leaves
// install-mode. `error` is a terminal failure with a retry.

import { FRAME_KIND, type LiveFrame } from "@/lib/api/events";

// The renderable install steps, in walk order. Pre-create steps (`importing`,
// `connect`) are client-driven from the import/create responses; the post-create
// steps below are advanced by the SSE `install:*` frames. `id` doubles as the
// stable React key and the value the tests assert on (RULE UFS — one source).
export const INSTALL_STEP = {
  IMPORTING: "importing",
  CONNECT: "connect",
  CREATING: "creating",
  PROVISIONING: "provisioning",
  READY: "ready",
  ERROR: "error",
} as const;

export type InstallStepId = (typeof INSTALL_STEP)[keyof typeof INSTALL_STEP];

// The post-create steps the SSE stream drives, lowest→highest rank. A frame
// only ever advances the rendered step forward (a duplicate or out-of-order
// frame never rewinds it) — `rankOf` powers that monotonic guard.
const SSE_STEP_ORDER: readonly InstallStepId[] = [
  INSTALL_STEP.CREATING,
  INSTALL_STEP.PROVISIONING,
  INSTALL_STEP.READY,
];

export function rankOf(step: InstallStepId): number {
  const idx = SSE_STEP_ORDER.indexOf(step);
  // Non-SSE steps (importing/connect/error) are not on the monotonic ladder;
  // -1 keeps them out of the forward-only comparison.
  return idx;
}

// Maps an SSE frame kind onto the install step it advances to, or null when the
// frame is not an install frame (the chat reducer handles those). Centralised so
// the registry and the tests agree on the kind→step contract verbatim.
export function installStepFromKind(kind: string): InstallStepId | null {
  switch (kind) {
    case FRAME_KIND.INSTALL_CREATING:
      return INSTALL_STEP.CREATING;
    case FRAME_KIND.INSTALL_PROVISIONING:
      return INSTALL_STEP.PROVISIONING;
    case FRAME_KIND.INSTALL_READY:
      return INSTALL_STEP.READY;
    case FRAME_KIND.INSTALL_ERROR:
      return INSTALL_STEP.ERROR;
    default:
      return null;
  }
}

// True for the four `install:*` discriminators — the registry uses this to fork
// install frames away from the chat-event path so they never become messages.
export function isInstallFrame(frame: LiveFrame): boolean {
  return installStepFromKind(frame.kind) !== null;
}

// Advance an install step in response to a frame, forward-only. `error` always
// wins (a failure is terminal regardless of where we were); otherwise a frame
// that maps to a lower-or-equal rank than the current step is ignored, so a
// late duplicate can never rewind the spinner.
export function advanceInstallStep(
  current: InstallStepId | null,
  next: InstallStepId,
): InstallStepId | null {
  if (next === INSTALL_STEP.ERROR) return INSTALL_STEP.ERROR;
  if (current === INSTALL_STEP.ERROR) return current;
  if (current === null) return next;
  return rankOf(next) > rankOf(current) ? next : current;
}

// The terminal step at which the fleet has gone active on the server and the UI
// must drop out of install-mode.
export function isInstallComplete(step: InstallStepId | null): boolean {
  return step === INSTALL_STEP.READY;
}
