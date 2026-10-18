// Forward only presentation metadata, never cookies or credential headers.

const PRIVATE = "private, no-store";
const CONTENT_TYPE = "Content-Type";
const JSON_TYPE = "application/json";
const PROBLEM = "application/problem+json";
const STREAM = "text/event-stream";
export const SETUP_REQUEST_ID_HEADER = "X-E2E-Request-Id";
const FORWARDED = ["ETag", "Location", "Retry-After", SETUP_REQUEST_ID_HEADER] as const;
export const SETUP_PROXY_DETAIL = {
  unavailable: "Setup resource unavailable",
  origin: "Cross-origin setup mutation refused",
  mediaType: "A setup mutation requires JSON",
  bodyLimit: "Setup body exceeds the native envelope limit",
  unauthenticated: "Not authenticated",
  unconfirmed: "Setup request could not be confirmed",
} as const;

export function setupProblem(status: number, detail: string): Response {
  return new Response(JSON.stringify({detail}), {status, headers: {[CONTENT_TYPE]: PROBLEM, "Cache-Control": PRIVATE}});
}

export function setupResponse(upstream: Response, stream: boolean, body: ReadableStream<Uint8Array> | null): Response {
  const type = upstream.headers.get(CONTENT_TYPE)?.split(";")[0]?.trim();
  const headers = new Headers({"Cache-Control": PRIVATE, Vary: "Cookie"});
  headers.set(CONTENT_TYPE, type === JSON_TYPE || type === PROBLEM || stream && type === STREAM ? type : "text/plain");
  if (stream) headers.set("X-Accel-Buffering", "no");
  for (const name of FORWARDED) {
    const value = upstream.headers.get(name);
    if (value !== null) headers.set(name, value);
  }
  return new Response(upstream.status === 204 ? null : body, {status: upstream.status, headers});
}
