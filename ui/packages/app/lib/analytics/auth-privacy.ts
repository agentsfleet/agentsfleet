const AUTH_ROUTE = /(?:^|\/)(?:cli-auth|sign-in|sign-up)(?:\/|[?#]|$)/;
const MAX_EVENT_VALUES = 1_000;
const MAX_DECODE_PASSES = 2;

export function isSensitiveAuthUrl(value: string): boolean {
  let decoded = value;
  for (let pass = 0; pass < MAX_DECODE_PASSES; pass++) {
    if (AUTH_ROUTE.test(decoded)) return true;
    try { decoded = decodeURIComponent(decoded); } catch { return false; }
  }
  return AUTH_ROUTE.test(decoded);
}

export function onAuthenticationRoute(): boolean {
  return typeof window !== "undefined" && isSensitiveAuthUrl(window.location.pathname);
}

// Applied at delivery as well as capture: SDK pageview/pageleave events and
// queued events can outlive navigation from the page that created them.
export function filterAuthenticationEvent<T>(event: T): T | null {
  if (onAuthenticationRoute()) return null;
  const pending: unknown[] = [event];
  const visited = new Set<object>();
  let inspected = 0;
  while (pending.length > 0) {
    if (++inspected > MAX_EVENT_VALUES) return null;
    const value = pending.pop();
    if (typeof value === "string" && isSensitiveAuthUrl(value)) return null;
    if (typeof value !== "object" || value === null || visited.has(value)) continue;
    visited.add(value);
    pending.push(...Object.values(value));
  }
  return event;
}
