/** Server-sent event helpers for specs that stand in for the fleet's live stream. */

export const SSE_CONTENT_TYPE = "text/event-stream";

export function sseFrame(kind: string, payload: Record<string, unknown>): string {
  return `event: ${kind}\ndata: ${JSON.stringify({ kind, ...payload })}\n\n`;
}
