import type { LiveFrame } from "@/lib/api/turns";
import type { FRAME_KIND } from "@/lib/api/turns-types";
import { advancesContext, readContextProgress } from "../context-progress";
import { AGENTSFLEET_TURN_STATUS, type FleetTurn } from "./fleet-stream-row";

/** A status hint can affect only its already-running row, never durable history. */
export function applyContextProgress(prev: FleetTurn[], frame: Extract<LiveFrame, { kind: typeof FRAME_KIND.CONTEXT_PROGRESS }>): FleetTurn[] {
  const progress = readContextProgress({ generation: frame.generation, state: frame.state });
  const index = prev.findIndex(turn => turn.id === frame.turn_id);
  const current = prev[index];
  if (!progress || !current || current.status !== AGENTSFLEET_TURN_STATUS.RECEIVED
    || !advancesContext(current.contextProgress, progress)) return prev;
  const next = [...prev];
  next[index] = { ...current, contextProgress: progress, contextProgressHidden: undefined };
  return next;
}

/** Reconnects discard the visible hint while retaining its ordering frontier. */
export function hideContextProgress(prev: FleetTurn[]): FleetTurn[] {
  const next = prev.map(turn => {
    if (!turn.contextProgress || turn.contextProgressHidden) return turn;
    return { ...turn, contextProgressHidden: true as const };
  });
  return next.some((turn, index) => turn !== prev[index]) ? next : prev;
}
