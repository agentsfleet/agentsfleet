/** Server-sent event helpers for specs that stand in for the fleet's live stream. */
import { OUTPUT_KINDS } from "@/lib/streaming/fleet-stream-attempt";

export const SSE_CONTENT_TYPE = "text/event-stream";

// The daemon stamps every output frame with its run's fence, as a decimal
// string (afd_fleet `run/activity/published.rs`), and the page drops an output
// frame without one. A stand-in frame is the first attempt's unless it names
// another fence itself.
const FIRST_ATTEMPT_FENCE = "1";

export function sseFrame(kind: string, payload: Record<string, unknown>): string {
  const fenced = OUTPUT_KINDS.has(kind) && !("fencing_token" in payload)
    ? { ...payload, fencing_token: FIRST_ATTEMPT_FENCE }
    : payload;
  return `event: ${kind}\ndata: ${JSON.stringify({ kind, ...fenced })}\n\n`;
}
