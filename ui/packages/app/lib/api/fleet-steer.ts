// The chat's steer, sent from the browser to the same-origin steer route,
// which mints the bearer and calls the daemon. A `fetch`, not a Server Action:
// Next runs one tab's Server Actions one at a time, so a hung steer held every
// later steer and every detail read behind it, and nothing could cancel it.
// The caller's signal cancels this one.
//
// Every outcome is a `SteerResult`, never a throw: an answer carries its
// status and code, and no answer — a transport failure or an abort — carries
// neither, which is how the caller knows the daemon may hold the message.

import { z } from "zod";
import { HTTP_STATUS_UNAUTHORIZED } from "./errors";
import { steerMessagesUrl, type SteerAccepted, type SteerRequest } from "./fleets-types";
import { ERROR_CODE } from "@/lib/errors";

/** A steer's answer: the daemon's 202, or a refusal's status and code. */
export type SteerResult =
  | { ok: true; data: SteerAccepted }
  | { ok: false; error: string; status?: number; errorCode?: string };

const CONTENT_TYPE_JSON = "application/json";
const NO_ANSWER = "The steer got no answer";
const NO_RECEIPT = "The steer was answered without a receipt";
const SIGNED_OUT = "Not authenticated";

// A daemon older than the app answers without `replayed`: it never replays, so
// a missing field reads as `false` rather than as no receipt, whichever of the
// two deploys first.
const AcceptedSchema = z.object({ status: z.string(), event_id: z.string(), replayed: z.boolean().default(false) });
// The route answers a refusal as the daemon's problem body does.
const RefusalSchema = z.object({ detail: z.string().optional(), error_code: z.string().optional() });

export async function postSteer(
  workspaceId: string,
  fleetId: string,
  message: string,
  operationId: string,
  signal: AbortSignal,
): Promise<SteerResult> {
  const body: SteerRequest = { message, operation_id: operationId };
  let res: Response;
  try {
    res = await fetch(steerMessagesUrl(workspaceId, fleetId), {
      method: "POST",
      headers: { "Content-Type": CONTENT_TYPE_JSON },
      body: JSON.stringify(body),
      signal,
      cache: "no-store",
      // The session check answers a signed-out request with a redirect to
      // sign-in. Followed, the sign-in page would read as the steer's answer.
      redirect: "manual",
    });
  } catch {
    return { ok: false, error: NO_ANSWER };
  }
  if (res.type === "opaqueredirect") {
    return { ok: false, error: SIGNED_OUT, status: HTTP_STATUS_UNAUTHORIZED, errorCode: ERROR_CODE.AUTH_401 };
  }
  return answerOf(res);
}

// A body cut off by the abort, or one that is not JSON, reads as none.
async function answerOf(res: Response): Promise<SteerResult> {
  const body: unknown = await res.json().catch(() => null);
  if (res.ok) {
    const accepted = AcceptedSchema.safeParse(body);
    // A success with no receipt proves nothing either way: it carries no
    // status, so the send reads as unconfirmed.
    return accepted.success ? { ok: true, data: accepted.data } : { ok: false, error: NO_RECEIPT };
  }
  const refusal = RefusalSchema.safeParse(body);
  const { detail, error_code: errorCode } = refusal.success ? refusal.data : {};
  return { ok: false, error: detail ?? res.statusText, status: res.status, errorCode };
}
