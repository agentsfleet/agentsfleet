// Server-only credentials authenticate every whitelisted native setup request.

import { credential } from "@/lib/auth/credential";
import { API_ORIGIN } from "@/lib/api/client";
import { StreamLifetime } from "@/lib/api/stream-lifetime";
import { matchSetupRoute, SETUP_METHOD } from "./path";
import { boundedBody, JSON_MEDIA_TYPE, sameOrigin } from "./body";
import { SETUP_PROXY_DETAIL, setupProblem, setupResponse } from "./response";

const REQUEST_TIMEOUT_MS = 30_000;
const FORWARDED_HEADERS = ["If-Match", "Idempotency-Key", "Last-Event-ID"] as const;
type Context = {readonly params: Promise<{workspaceId: string; setupPath: string[]}>};

export async function proxySetup(request: Request, context: Context): Promise<Response> {
  const {workspaceId, setupPath} = await context.params;
  const route = matchSetupRoute(workspaceId, setupPath, request.method);
  if (!route) return setupProblem(404, SETUP_PROXY_DETAIL.unavailable);
  const writing = request.method !== SETUP_METHOD.get;
  if (writing && !sameOrigin(request)) return setupProblem(403, SETUP_PROXY_DETAIL.origin);
  let lifetime: StreamLifetime | undefined;
  const signal = route.stream ? request.signal : AbortSignal.any([request.signal, AbortSignal.timeout(REQUEST_TIMEOUT_MS)]);
  try {
    let body: Uint8Array<ArrayBuffer> | undefined;
    if (writing && request.method !== SETUP_METHOD.delete && !route.bodyless) {
      if (request.headers.get("content-type")?.split(";")[0]?.trim().toLowerCase() !== JSON_MEDIA_TYPE) {
        return setupProblem(415, SETUP_PROXY_DETAIL.mediaType);
      }
      const bounded = await boundedBody(request, signal);
      if (bounded === null) return setupProblem(413, SETUP_PROXY_DETAIL.bodyLimit);
      body = bounded;
    }
    const token = await credential();
    if (!token) return setupProblem(401, SETUP_PROXY_DETAIL.unauthenticated);
    const headers = new Headers({Authorization: `Bearer ${token}`, Accept: route.stream ? "text/event-stream" : JSON_MEDIA_TYPE});
    if (body) headers.set("Content-Type", JSON_MEDIA_TYPE);
    for (const name of FORWARDED_HEADERS) {
      const value = request.headers.get(name);
      if (value !== null) headers.set(name, value);
    }
    lifetime = route.stream ? new StreamLifetime(request.signal) : undefined;
    const upstream = await fetch(target(request, workspaceId, setupPath, route.query), {
      method: request.method, headers, signal: lifetime?.signal ?? signal, cache: "no-store", redirect: "error", ...(body ? {body} : {}),
    });
    return setupResponse(upstream, route.stream, lifetime ? lifetime.wrap(upstream.body) : upstream.body);
  } catch {
    lifetime?.dispose();
    return setupProblem(502, SETUP_PROXY_DETAIL.unconfirmed);
  }
}

function target(request: Request, workspace: string, parts: readonly string[], keys: readonly string[]): string {
  const path = ["v1", "workspaces", workspace, ...parts].map(encodeURIComponent).join("/");
  const query = new URLSearchParams();
  const incoming = new URL(request.url).searchParams;
  for (const name of keys) {
    const value = incoming.get(name);
    if (value !== null) query.set(name, value);
  }
  return `${API_ORIGIN}/${path}${query.size ? `?${query}` : ""}`;
}
