import { cookies } from "next/headers";
import { credential } from "@/lib/auth/credential";
import { requireApiOrigin } from "@/lib/api/client";
import {
  CONNECTOR_CALLBACK_TIMEOUT_MS, CONNECTOR_COMPLETION_PATH,
  CONNECTOR_RETURN_COOKIE, connectorReturnPath,
  type CONNECTOR_FAILURE_COPY,
} from "@/lib/connectors/completion";

type Params = { params: Promise<{ provider: string }> };
type FailureReason = keyof typeof CONNECTOR_FAILURE_COPY;

const PROVIDER_ID_PATTERN = /^[a-z][a-z0-9_-]{0,63}$/;
const COMPLETE_PATH_PREFIX = "/v1/connectors/";
const COMPLETE_PATH_SUFFIX = "/callback";
const AUTHORIZATION_HEADER = "Authorization";
const BEARER_PREFIX = "Bearer ";
const REDIRECT_STATUS = 302;
const INVALID_PROVIDER_STATUS = 400;
const STATE_REFUSED_STATUS = 400;

export const runtime = "nodejs";
export const dynamic = "force-dynamic";

// Only the backend verifies state, consumes its nonce and persists the grant.
// A failure always starts a fresh flow; this relay never retries a spent code.
export async function GET(req: Request, { params }: Params): Promise<Response> {
  const { provider } = await params;
  if (!PROVIDER_ID_PATTERN.test(provider)) {
    return Response.json({ error: "Invalid connector provider" }, { status: INVALID_PROVIDER_STATUS });
  }
  const controller = new AbortController();
  let deadline: ReturnType<typeof setTimeout> | undefined;
  const timeout = new Promise<never>((_resolve, reject) => {
    deadline = setTimeout(() => {
      controller.abort();
      reject(controller.signal.reason);
    }, CONNECTOR_CALLBACK_TIMEOUT_MS);
  });
  try {
    return await Promise.race([relay(req, provider, controller.signal), timeout]);
  } catch {
    return failed(req, provider, controller.signal.aborted ? "timeout" : "unavailable");
  } finally {
    clearTimeout(deadline);
  }
}

async function relay(req: Request, provider: string, signal: AbortSignal): Promise<Response> {
  const token = await credential();
  // A late credential answer must not redeem a code after the recovery page.
  signal.throwIfAborted();
  if (!token) return failed(req, provider, "session");
  if (new URL(req.url).searchParams.has("error")) return failed(req, provider, "denied");
  const upstream = new URL(`${COMPLETE_PATH_PREFIX}${provider}${COMPLETE_PATH_SUFFIX}`, requireApiOrigin());
  upstream.search = new URL(req.url).search;
  const response = await fetch(upstream, {
    headers: { [AUTHORIZATION_HEADER]: `${BEARER_PREFIX}${token}` },
    method: "POST", redirect: "manual", signal,
  });
  // The body is never shown or logged. Vendor bodies can contain secrets.
  await response.body?.cancel();
  signal.throwIfAborted();
  if (response.status !== REDIRECT_STATUS) {
    return failed(req, provider, response.status === STATE_REFUSED_STATUS ? "expired" : "unavailable");
  }
  return completed(req, provider, response.headers.get("location"));
}

async function failed(req: Request, provider: string, reason: FailureReason): Promise<Response> {
  const jar = await cookies();
  const name = `${CONNECTOR_RETURN_COOKIE}${provider}`;
  const destination = connectorReturnPath(jar.get(name)?.value);
  jar.delete(name);
  const landing = new URL(CONNECTOR_COMPLETION_PATH, req.url);
  landing.searchParams.set("reason", reason);
  if (destination) landing.searchParams.set("return_to", destination);
  return Response.redirect(landing, REDIRECT_STATUS);
}

async function completed(req: Request, provider: string, location: string | null): Promise<Response> {
  const destination = location ? new URL(location, req.url) : null;
  if (!destination || destination.origin !== new URL(req.url).origin
      || !connectorReturnPath(destination.pathname)) {
    return failed(req, provider, "unavailable");
  }
  destination.search = "";
  destination.hash = "";
  (await cookies()).delete(`${CONNECTOR_RETURN_COOKIE}${provider}`);
  return Response.redirect(destination, REDIRECT_STATUS);
}
