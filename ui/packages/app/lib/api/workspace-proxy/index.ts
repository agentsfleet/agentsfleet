// Server-only credentials authenticate every whitelisted workspace request.

import { credential } from "@/lib/auth/credential";
import { API_ORIGIN } from "@/lib/api/client";
import { matchWorkspaceRoute, PROXY_METHOD } from "./path";
import { boundedBody, JSON_MEDIA_TYPE, sameOrigin } from "./body";
import { PROXY_DETAIL, proxyProblem, proxyResponse } from "./response";

const FORWARDED_HEADERS = ["If-Match", "Idempotency-Key"] as const;
type Context = {readonly params: Promise<{workspaceId: string; resourcePath: string[]}>};

export async function proxyWorkspace(request: Request, context: Context): Promise<Response> {
  const {workspaceId, resourcePath} = await context.params;
  const route = matchWorkspaceRoute(workspaceId, resourcePath, request.method);
  if (!route) return proxyProblem(404, PROXY_DETAIL.unavailable);
  const writing = request.method !== PROXY_METHOD.get;
  if (writing && !sameOrigin(request)) return proxyProblem(403, PROXY_DETAIL.origin);
  const signal = request.signal;
  try {
    let body: Uint8Array<ArrayBuffer> | undefined;
    if (writing && request.method !== PROXY_METHOD.delete && !route.bodyless) {
      if (request.headers.get("content-type")?.split(";")[0]?.trim().toLowerCase() !== JSON_MEDIA_TYPE) {
        return proxyProblem(415, PROXY_DETAIL.mediaType);
      }
      const bounded = await boundedBody(request, signal, route.bodyLimit);
      if (bounded === null) return proxyProblem(413, PROXY_DETAIL.bodyLimit);
      body = bounded;
    }
    const token = await credential();
    if (!token) return proxyProblem(401, PROXY_DETAIL.unauthenticated);
    const headers = new Headers({Authorization: `Bearer ${token}`, Accept: JSON_MEDIA_TYPE});
    if (body) headers.set("Content-Type", JSON_MEDIA_TYPE);
    for (const name of FORWARDED_HEADERS) {
      const value = request.headers.get(name);
      if (value !== null) headers.set(name, value);
    }
    const upstream = await fetch(target(request, workspaceId, resourcePath, route.query), {
      method: request.method, headers, signal, cache: "no-store", redirect: "error", ...(body ? {body} : {}),
    });
    return proxyResponse(upstream, upstream.body);
  } catch {
    return proxyProblem(502, PROXY_DETAIL.unconfirmed);
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
