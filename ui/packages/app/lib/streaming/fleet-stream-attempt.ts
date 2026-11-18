import type { LiveFrame } from "@/lib/api/events";
import type { Entry } from "./fleet-stream-entry";
import type { ApplyEvents } from "./fleet-stream-detail-recovery";
import * as v from "valibot";
import { FRAME_KIND } from "@/lib/api/events-types";
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
export function acceptAttemptFrame(entry: Entry, frame: LiveFrame, apply: ApplyEvents): boolean {
  if (!OUTPUT_KINDS.has(frame.kind)) return true;
  if (!("event_id" in frame) || typeof frame.event_id !== "string") return false;
  const eventId = frame.event_id;
  if (entry.completedReplies.has(eventId) || entry.snapshot.events.some(event =>
    event.id === eventId && TERMINAL_STATUSES.has(event.status))) return false;
  const parsed = v.safeParse(FENCE, frame.fencing_token);
  if (!parsed.success) return false;
  const fence = parsed.output;
  const previous = entry.replyAttempts.get(eventId);
  if (previous !== undefined && (fence.length < previous.length
    || (fence.length === previous.length && fence < previous))) return false;
  if (fence !== previous) {
    entry.replyAttempts.set(eventId, fence);
    if (previous !== undefined || frame.kind === FRAME_KIND.RUN_RESTARTED) restart(entry, eventId, fence, apply);
  }
  return frame.kind !== FRAME_KIND.RUN_RESTARTED;
}

function restart(entry: Entry, eventId: string, fence: string, apply: ApplyEvents): void {
  closeLivePass(entry, eventId);
  entry.replyGaps.delete(eventId);
  entry.replyRecoveries.delete(eventId);
  entry.runningReads.delete(eventId);
  entry.bodyReads.delete(eventId);
  entry.traceReads.delete(eventId);
  apply(prev => applyLiveFrame(prev, {
    kind: FRAME_KIND.CHUNK, event_id: eventId, text_kind: "answer", text: "",
  }).map(event => event.id === eventId ? {
    ...interruptOpenCalls(event), runRestarted: true, restartAttempt: fence, reply: "", reasoning: "",
    thinking: false, reasoningStartedAtMs: undefined, reasoningEndedAtMs: undefined,
    replyRecovering: false, contextProgress: undefined, contextProgressHidden: undefined,
    taskActivity: undefined,
  } : event), {});
}
