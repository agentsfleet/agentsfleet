import type { LiveFrame } from "@/lib/api/turns";
import type { Entry } from "./fleet-stream-entry";
import type { ApplyTurns } from "./fleet-stream-detail-recovery";
import * as v from "valibot";
import { FRAME_KIND } from "@/lib/api/turns-types";
import { closeLivePass } from "./fleet-stream-detail-recovery";
import { applyLiveFrame } from "./fleet-stream-frames";
import { TERMINAL_STATUSES } from "./fleet-stream-row";
import { interruptOpenCalls } from "./fleet-stream-tool-trace";

const MAX_FENCE = "9223372036854775807";
const FENCE = v.pipe(v.string(), v.regex(/^[1-9][0-9]{0,18}$/u),
  v.check(value => value.length < MAX_FENCE.length || value <= MAX_FENCE));
const OUTPUT_KINDS: ReadonlySet<string> = new Set([
  FRAME_KIND.CHUNK, FRAME_KIND.TOOL_CALL_STARTED, FRAME_KIND.TOOL_CALL_PROGRESS,
  FRAME_KIND.TOOL_CALL_COMPLETED, FRAME_KIND.CONTEXT_PROGRESS,
  FRAME_KIND.TASK_DECISION_STARTED, FRAME_KIND.TASK_DECISION_COMPLETED,
  FRAME_KIND.TASK_TRANSFER_PROGRESS, FRAME_KIND.RUN_RESTARTED,
]);

/** Compare canonical decimal strings without losing a database bigint's precision. */
export function acceptAttemptFrame(entry: Entry, frame: LiveFrame, apply: ApplyTurns): boolean {
  if (!OUTPUT_KINDS.has(frame.kind)) return true;
  if (!("turn_id" in frame) || typeof frame.turn_id !== "string") return false;
  const turnId = frame.turn_id;
  if (entry.completedReplies.has(turnId) || entry.snapshot.turns.some(turn =>
    turn.id === turnId && TERMINAL_STATUSES.has(turn.status))) return false;
  const parsed = v.safeParse(FENCE, frame.fencing_token);
  if (!parsed.success) return false;
  const fence = parsed.output;
  const previous = entry.replyAttempts.get(turnId);
  if (previous !== undefined && (fence.length < previous.length
    || (fence.length === previous.length && fence < previous))) return false;
  if (fence !== previous) {
    entry.replyAttempts.set(turnId, fence);
    if (previous !== undefined || frame.kind === FRAME_KIND.RUN_RESTARTED) restart(entry, turnId, fence, apply);
  }
  return frame.kind !== FRAME_KIND.RUN_RESTARTED;
}

function restart(entry: Entry, turnId: string, fence: string, apply: ApplyTurns): void {
  closeLivePass(entry, turnId);
  entry.replyGaps.delete(turnId);
  entry.replyRecoveries.delete(turnId);
  entry.runningReads.delete(turnId);
  entry.bodyReads.delete(turnId);
  entry.traceReads.delete(turnId);
  apply(prev => applyLiveFrame(prev, {
    kind: FRAME_KIND.CHUNK, turn_id: turnId, text_kind: "answer", text: "",
  }).map(turn => turn.id === turnId ? {
    ...interruptOpenCalls(turn), runRestarted: true, restartAttempt: fence, reply: "", reasoning: "",
    thinking: false, reasoningStartedAtMs: undefined, reasoningEndedAtMs: undefined,
    replyRecovering: false, contextProgress: undefined, contextProgressHidden: undefined,
    taskActivity: undefined,
  } : turn), {});
}
