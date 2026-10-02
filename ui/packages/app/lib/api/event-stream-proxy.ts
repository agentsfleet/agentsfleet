// What the two same-origin stream proxies answer an EventSource with: the
// workspace stream (app/live/v1/workspaces/[workspaceId]/events/stream) and
// the per-fleet one beneath it. Neither may import the API client, which their
// tests replace wholesale, so this reads only the frame vocabulary and codes.

import { z } from "zod";
import { FRAME_KIND } from "@/lib/api/events-types";
import { ERROR_CODE } from "@/lib/errors";

const HTTP_STATUS_OK = 200;
const HTTP_STATUS_FORBIDDEN = 403;

const EVENT_STREAM_HEADERS = {
  "Content-Type": "text/event-stream",
  "Cache-Control": "no-cache, no-transform",
  Connection: "keep-alive",
  // Defend against intermediary buffering (nginx, etc.) that would bunch
  // frames and defeat the live-tail UX.
  "X-Accel-Buffering": "no",
} as const;

// `Frame::access_revoked` in rustd/crates/afd_sse/src/frame.rs, written the way
// rustd/crates/afd_api_tenant/src/handler/stream/body.rs puts a control frame
// on the wire: sequence zero, the kind as the event name, `kind` first.
const ACCESS_REVOKED_FRAME =
  `id: 0\nevent: ${FRAME_KIND.ACCESS_REVOKED}\n` +
  `data: ${JSON.stringify({ kind: FRAME_KIND.ACCESS_REVOKED, error_code: ERROR_CODE.AUTH_FORBIDDEN })}\n\n`;

// The one field of the daemon's problem body this module reads.
const RefusalSchema = z.object({ error_code: z.string() });

/** A live stream, with the headers an EventSource and every hop need. */
export function eventStreamResponse(body: BodyInit): Response {
  return new Response(body, { status: HTTP_STATUS_OK, headers: EVENT_STREAM_HEADERS });
}

/**
 * Whether the daemon refused to open a stream because the caller has no access
 * to its workspace: 403 `UZ-AUTH-001`, the code `access_revoked` carries when
 * access goes mid-stream. Any other refusal, or a body that is not the
 * daemon's problem JSON, is not this one.
 */
export function isAccessRefusal(status: number, body: string): boolean {
  if (status !== HTTP_STATUS_FORBIDDEN) return false;
  const refusal = RefusalSchema.safeParse(parseJson(body));
  return refusal.success && refusal.data.error_code === ERROR_CODE.AUTH_FORBIDDEN;
}

/**
 * The refusal, said as the frame that ends a stream whose caller lost access.
 * An EventSource refused at open fires a bare `error` with no status, which
 * both registries read as an outage and retry for good; this reaches their
 * `access_revoked` listener instead, and that one is terminal.
 */
export function accessRevokedResponse(): Response {
  return eventStreamResponse(ACCESS_REVOKED_FRAME);
}

function parseJson(text: string): unknown {
  try {
    return JSON.parse(text);
  } catch {
    return null;
  }
}
