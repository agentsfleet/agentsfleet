// Forward only presentation metadata, never cookies or credential headers.

const PRIVATE = "private, no-store";
const CONTENT_TYPE = "Content-Type";
const JSON_TYPE = "application/json";
const PROBLEM = "application/problem+json";
export const PROXY_REQUEST_ID_HEADER = "X-E2E-Request-Id";
const FORWARDED = ["ETag", "Location", "Retry-After", PROXY_REQUEST_ID_HEADER] as const;
export const PROXY_DETAIL = {
  unavailable: "Workspace resource unavailable",
  origin: "Cross-origin workspace mutation refused",
  mediaType: "A workspace mutation requires JSON",
  bodyLimit: "The body exceeds what the daemon reads",
  unauthenticated: "Not authenticated",
  unconfirmed: "The request could not be confirmed",
} as const;

export function proxyProblem(status: number, detail: string): Response {
  return new Response(JSON.stringify({detail}), {status, headers: {[CONTENT_TYPE]: PROBLEM, "Cache-Control": PRIVATE}});
}

export function proxyResponse(upstream: Response, body: ReadableStream<Uint8Array> | null): Response {
  const type = upstream.headers.get(CONTENT_TYPE)?.split(";")[0]?.trim();
  const headers = new Headers({"Cache-Control": PRIVATE, Vary: "Cookie"});
  headers.set(CONTENT_TYPE, type === JSON_TYPE || type === PROBLEM ? type : "text/plain");
  for (const name of FORWARDED) {
    const value = upstream.headers.get(name);
    if (value !== null) headers.set(name, value);
  }
  return new Response(upstream.status === 204 ? null : body, {status: upstream.status, headers});
}
