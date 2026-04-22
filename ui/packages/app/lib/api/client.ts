import { ApiError } from "./errors";

// Full backend origin — used for display URLs (webhooks) and server-side fetches.
export const API_ORIGIN = process.env.NEXT_PUBLIC_API_URL ?? "https://api-dev.usezombie.com";

// BASE for fetch calls. On the server we hit the backend directly (no CORS).
// In the browser we go through the same-origin `/backend` proxy configured in
// next.config.ts `rewrites` — browser never sees a cross-origin request.
export const BASE = typeof window === "undefined" ? API_ORIGIN : "/backend";

export async function request<T>(
  path: string,
  init: RequestInit,
  token: string,
): Promise<T> {
  const res = await fetch(`${BASE}${path}`, {
    ...init,
    headers: {
      "Content-Type": "application/json",
      Authorization: `Bearer ${token}`,
      ...init.headers,
    },
  });

  if (res.status === 204) return undefined as T;

  const body = await res.json().catch(() => ({ error: res.statusText }));

  if (!res.ok) {
    throw new ApiError(
      body.error ?? res.statusText,
      res.status,
      body.code ?? "UZ-UNKNOWN",
      body.request_id,
    );
  }

  return body as T;
}
