import type { LiveFrame } from "@/lib/api/turns";
import { FRAME_KIND } from "@/lib/api/turns-types";

/** The four `performance` marks one send leaves, each naming the send's
 * operation id in its detail, so overlapping sends never share a mark. */
export const SEND_MARK = {SEND: "af:send", ACCEPTED: "af:accepted", FIRST_FRAME: "af:first-frame", COMPLETE: "af:complete"} as const;

/** How many accepted sends wait for their frames at once. A send whose
 * completion never comes is forgotten oldest first past it. */
export const SEND_MARKS_MAX = 64;

type LeadMark = typeof SEND_MARK.SEND | typeof SEND_MARK.ACCEPTED;
type AcceptedSend = {operationId: string; heard: boolean};

/** This tab's accepted sends, by the turn each 202 named, until that turn's
 * completion leaves its last mark. */
export class SendMarks {
  #sends = new Map<string, AcceptedSend>();

  /** Marks a send leaving, or its 202; an accepted send waits for `turnId`'s frames. */
  mark(name: LeadMark, operationId: string, turnId?: string): void {
    if (turnId !== undefined) {
      this.#sends.set(turnId, {operationId, heard: false});
      for (const oldest of this.#sends.keys()) {
        if (this.#sends.size <= SEND_MARKS_MAX) break;
        this.#sends.delete(oldest);
      }
    }
    leaveMark(name, operationId);
  }

  /** Marks the first frame and the completion of an accepted send's turn. A
   * send's own frames wait for its 202, so these follow `af:accepted`. */
  frame(frame: LiveFrame): void {
    // A gate resolved outside a run names no turn.
    const turnId = "turn_id" in frame ? frame.turn_id : null;
    if (turnId === null) return;
    const sent = this.#sends.get(turnId);
    if (sent === undefined) return;
    if (!sent.heard) leaveMark(SEND_MARK.FIRST_FRAME, sent.operationId);
    sent.heard = true;
    if (frame.kind !== FRAME_KIND.TURN_COMPLETE) return;
    this.#sends.delete(turnId);
    leaveMark(SEND_MARK.COMPLETE, sent.operationId);
  }

  /** Forgets every accepted send. */
  reset(): void {
    this.#sends.clear();
  }
}

/** The tab's one set: the send hook marks into it, the stream dispatch reads it. */
export const sendMarks = new SendMarks();

// A timing aid never breaks what it times. `mark` runs in the send hook and
// `frame` runs in the stream dispatch ahead of the chunk write, so a browser
// that has no User Timing, or refuses a mark, loses the mark and nothing else.
function leaveMark(name: string, operationId: string): void {
  try {
    performance.mark(name, {detail: {operationId}});
  } catch {
    // The mark is optional; the send it would describe is not.
  }
}
