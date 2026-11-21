import type { LiveFrame } from "@/lib/api/turns";
import { FRAME_KIND } from "@/lib/api/turns-types";
import { SEND_MARK } from "@/lib/streaming/send-marks";

/** A send's marks in the order its frames reach this tab. */
export const IN_ORDER = [SEND_MARK.SEND, SEND_MARK.ACCEPTED, SEND_MARK.FIRST_FRAME, SEND_MARK.COMPLETE];

/** A piece of `turnId`'s streamed answer. */
export function chunkOf(turnId: string): LiveFrame {
  return { kind: FRAME_KIND.CHUNK, turn_id: turnId, text: "…", text_kind: "answer" };
}

/** `turnId`'s completion. */
export function completionOf(turnId: string): LiveFrame {
  return { kind: FRAME_KIND.TURN_COMPLETE, turn_id: turnId, status: "processed", final_reply: "" };
}

/** Every send mark left so far, as (name, operation id), in the order left. */
export function markedSends(): Array<[string, string]> {
  const names = new Set<string>(Object.values(SEND_MARK));
  const marks: Array<[string, string]> = [];
  for (const entry of performance.getEntriesByType("mark")) {
    if (!(entry instanceof PerformanceMark) || !names.has(entry.name)) continue;
    const detail: unknown = entry.detail;
    if (typeof detail === "object" && detail !== null && "operationId" in detail && typeof detail.operationId === "string") {
      marks.push([entry.name, detail.operationId]);
    }
  }
  return marks;
}
