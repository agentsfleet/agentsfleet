// Same-origin steer proxy. The browser holds no bearer, so the chat's send
// POSTs here and this handler mints the API-audience token server-side, as
// the events proxies beside it do (docs/AUTH.md "SSE stream"), then calls the
// daemon through `steerFleet` — the operation id rides in the body, so the
// policy's socket-drop replay re-sends the same operation.
//
// It replaces a Server Action: Next runs one tab's Server Actions one at a
// time, so a hung steer held every later steer and every detail read, and the
// browser could not cancel it. A Server Action also carried Next's origin
// check; a Route Handler carries none, so this one refuses what a cross-site
// page could send — a POST whose `Origin` is absent or foreign, a body that is
// not JSON, and a body larger than any steer can be.

import { z } from "zod";
import { ApiError } from "@/lib/api/errors";
import { steerFleet } from "@/lib/api/fleets";
import { STEER_MESSAGE_MAX_BYTES, type SteerRequest } from "@/lib/api/fleets-types";
import { credential } from "@/lib/auth/credential";
import { ERROR_CODE } from "@/lib/errors";

export const runtime = "nodejs";
export const dynamic = "force-dynamic";

type Params = {
  params: Promise<{ workspaceId: string; fleetId: string }>;
};

const CONTENT_TYPE_JSON = "application/json";
const CONTENT_TYPE_PROBLEM = "application/problem+json";
// Authed per-tenant answers must never land in a shared cache.
const CACHE_CONTROL_NO_STORE = "no-store";
// encodeURIComponent leaves '.' intact, so a bare '..'/'.' path param would
// dot-normalize inside fetch and steer the minted token at another path.
const DOT_ONLY_SEGMENT = /^\.+$/;
// A sandboxed or `file:` page sends this literal Origin.
const OPAQUE_ORIGIN = "null";

// The largest body a steer can be: JSON's widest escape spends six bytes on
// one (`\u0000`), over the daemon's longest message and operation id, plus
// the two keys and their punctuation.
const JSON_ESCAPE_MAX_BYTES = 6;
const OPERATION_ID_MAX_BYTES = 200;
const ENVELOPE_BYTES = 64;
const STEER_BODY_MAX_BYTES =
  (STEER_MESSAGE_MAX_BYTES + OPERATION_ID_MAX_BYTES) * JSON_ESCAPE_MAX_BYTES + ENVELOPE_BYTES;

const HTTP_STATUS = {
  ACCEPTED: 202,
  BAD_REQUEST: 400,
  UNAUTHORIZED: 401,
  FORBIDDEN: 403,
  PAYLOAD_TOO_LARGE: 413,
  UNSUPPORTED_MEDIA_TYPE: 415,
  BAD_GATEWAY: 502,
} as const;
const HTTP_STATUS_ERROR_FLOOR = 400;
const HTTP_STATUS_ERROR_CEILING = 599;

const REFUSAL = {
  CROSS_ORIGIN: "Cross-origin steer refused",
  BAD_PATH: "Invalid path parameter",
  NOT_JSON: "A steer is a JSON body",
  TOO_LARGE: "A steer body cannot be this large",
  MALFORMED: "A steer carries a message and an operation id",
  UNAUTHORIZED: "Not authenticated",
  UNREACHABLE: "Upstream unreachable",
} as const;

// Mirrors the daemon's parser, which refuses unknown fields.
const SteerBodySchema = z.object({ message: z.string(), operation_id: z.string() }).strict();

export async function POST(req: Request, { params }: Params): Promise<Response> {
  if (!sameOrigin(req)) return problem(HTTP_STATUS.FORBIDDEN, REFUSAL.CROSS_ORIGIN);
  const { workspaceId, fleetId } = await params;
  if (DOT_ONLY_SEGMENT.test(workspaceId) || DOT_ONLY_SEGMENT.test(fleetId)) {
    return problem(HTTP_STATUS.BAD_REQUEST, REFUSAL.BAD_PATH);
  }
  const steer = await readSteer(req);
  if (steer instanceof Response) return steer;
  const token = await credential();
  if (!token) return problem(HTTP_STATUS.UNAUTHORIZED, REFUSAL.UNAUTHORIZED, ERROR_CODE.AUTH_401);
  try {
    const accepted = await steerFleet(workspaceId, fleetId, steer, token);
    return json(HTTP_STATUS.ACCEPTED, accepted);
  } catch (error) {
    return passthrough(error);
  }
}

// The host this request was addressed to, read as Next's own Server Action
// check reads it: the proxy's forwarded host first, then `Host`.
function sameOrigin(req: Request): boolean {
  const origin = req.headers.get("origin");
  if (origin === null || origin === OPAQUE_ORIGIN) return false;
  const forwarded = req.headers.get("x-forwarded-host")?.split(",")[0]?.trim();
  const host = forwarded || req.headers.get("host") || new URL(req.url).host;
  try {
    return new URL(origin).host === host;
  } catch {
    return false;
  }
}

// The steer in the body, or the refusal of a body that cannot be one.
async function readSteer(req: Request): Promise<SteerRequest | Response> {
  const mediaType = req.headers.get("content-type")?.split(";")[0]?.trim().toLowerCase();
  if (mediaType !== CONTENT_TYPE_JSON) return problem(HTTP_STATUS.UNSUPPORTED_MEDIA_TYPE, REFUSAL.NOT_JSON);
  const text = await boundedText(req);
  if (text === null) return problem(HTTP_STATUS.PAYLOAD_TOO_LARGE, REFUSAL.TOO_LARGE);
  let parsed: unknown;
  try {
    parsed = JSON.parse(text);
  } catch {
    return problem(HTTP_STATUS.BAD_REQUEST, REFUSAL.MALFORMED);
  }
  const steer = SteerBodySchema.safeParse(parsed);
  return steer.success ? steer.data : problem(HTTP_STATUS.BAD_REQUEST, REFUSAL.MALFORMED);
}

// The body as text, read no further than a steer can reach: null past that,
// whether the length was declared or only found while reading.
async function boundedText(req: Request): Promise<string | null> {
  if (Number(req.headers.get("content-length") ?? 0) > STEER_BODY_MAX_BYTES) return null;
  const reader = req.body?.getReader();
  if (reader === undefined) return "";
  const decoder = new TextDecoder();
  let size = 0;
  let text = "";
  for (let chunk = await reader.read(); !chunk.done; chunk = await reader.read()) {
    size += chunk.value.byteLength;
    if (size > STEER_BODY_MAX_BYTES) {
      await reader.cancel();
      return null;
    }
    text += decoder.decode(chunk.value, { stream: true });
  }
  return text + decoder.decode();
}

// The daemon's refusal, passed through with its status and code. Anything
// that is not an answer — a socket that never connected, a cancel — is a 502:
// the browser then reads the send as unconfirmed, not refused.
function passthrough(error: unknown): Response {
  if (!(error instanceof ApiError)) return problem(HTTP_STATUS.BAD_GATEWAY, REFUSAL.UNREACHABLE);
  const answered = error.status >= HTTP_STATUS_ERROR_FLOOR && error.status <= HTTP_STATUS_ERROR_CEILING;
  return problem(answered ? error.status : HTTP_STATUS.BAD_GATEWAY, error.message, error.code, error.requestId);
}

function problem(status: number, detail: string, errorCode?: string, requestId?: string): Response {
  const body = { detail, ...(errorCode ? { error_code: errorCode } : {}), ...(requestId ? { request_id: requestId } : {}) };
  return new Response(JSON.stringify(body), { status, headers: headersFor(CONTENT_TYPE_PROBLEM) });
}

function json(status: number, body: unknown): Response {
  return new Response(JSON.stringify(body), { status, headers: headersFor(CONTENT_TYPE_JSON) });
}

function headersFor(contentType: string): HeadersInit {
  return { "Content-Type": contentType, "Cache-Control": CACHE_CONTROL_NO_STORE };
}
