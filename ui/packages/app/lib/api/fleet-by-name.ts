// One fleet in the workspace by its saved name, read by the browser through the
// same-origin setup proxy (`GET /v1/workspaces/{id}/fleets?name=`), which mints
// the bearer the browser does not hold. An addressed chat message resolves its
// name here before anything is sent.
//
// Every outcome is a `FleetLookup`, never a throw, in the shape a steer's answer
// has (`fleet-steer.ts`): a refusal carries its status, and no answer carries
// none, so the send maps a failed lookup as it maps a failed steer.

import { HTTP_STATUS_UNAUTHORIZED } from "./errors";
import { FleetListSchema } from "./fleet-detail";
import { boundedBody } from "./setup-proxy/body";
import { ERROR_CODE } from "@/lib/errors";

/** The fleet saved under the name (null when none is), or why the lookup has no answer. */
export type FleetLookup =
  | { ok: true; fleetId: string | null }
  | { ok: false; error: string; status?: number; errorCode?: string };

const QUERY_NAME = "name";
const NO_ANSWER = "The fleet lookup got no answer";
const NO_LIST = "The fleet lookup was answered without a fleet list";
const SIGNED_OUT = "Not authenticated";
// The answer holds one fleet's summary at most. A daemon that predates the
// filter ignores it and answers a page of fleets, which this still holds.
const ANSWER_LIMIT_BYTES = 256 * 1024;

export async function findFleetByName(workspaceId: string, name: string, signal: AbortSignal): Promise<FleetLookup> {
  let res: Response;
  try {
    res = await fetch(fleetByNameUrl(workspaceId, name), {
      method: "GET",
      headers: { Accept: "application/json" },
      signal,
      cache: "no-store",
      // The session check answers a signed-out request with a redirect to
      // sign-in. Followed, the sign-in page would read as the lookup's answer.
      redirect: "manual",
    });
  } catch {
    return { ok: false, error: NO_ANSWER };
  }
  if (res.type === "opaqueredirect") {
    return { ok: false, error: SIGNED_OUT, status: HTTP_STATUS_UNAUTHORIZED, errorCode: ERROR_CODE.AUTH_401 };
  }
  if (!res.ok) return { ok: false, error: res.statusText, status: res.status };
  const list = FleetListSchema.safeParse(await jsonOf(res, signal));
  if (!list.success) return { ok: false, error: NO_LIST };
  // Only a fleet saved under exactly this name answers to it: a daemon that
  // ignored the filter answered other fleets, and none of them may take words
  // addressed to another.
  return { ok: true, fleetId: list.data.items.find((fleet) => fleet.name === name)?.id ?? null };
}

function fleetByNameUrl(workspaceId: string, name: string): string {
  const query = new URLSearchParams({ [QUERY_NAME]: name });
  return `/live/v1/workspaces/${encodeURIComponent(workspaceId)}/fleets?${query}`;
}

// The body as JSON, or null when it was cut off, ran past its limit or is not JSON.
async function jsonOf(res: Response, signal: AbortSignal): Promise<unknown> {
  try {
    const body = await boundedBody(res, signal, ANSWER_LIMIT_BYTES);
    return body === null ? null : JSON.parse(new TextDecoder().decode(body));
  } catch {
    return null;
  }
}
