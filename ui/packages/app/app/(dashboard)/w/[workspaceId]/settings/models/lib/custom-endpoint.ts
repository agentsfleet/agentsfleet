import { HTTPS_SCHEME_PREFIX } from "@/lib/types";

// Shared custom-endpoint client validation, kept independent of any one form
// component so the registry Add dialog's custom-endpoint shape can reuse it.

export const BASE_URL_NOT_HTTPS = "Use https:// for the Base URL.";

// Client-side https gate, matching the CLI option validator and the server-side
// guard's first check. Parses as a URL so a malformed value is caught for the
// same reason rather than slipping through a bare prefix test. The server
// re-validates and additionally blocks SSRF-unsafe hosts (loopback / private /
// metadata) — this is only the cheap, name-the-reason inline check.
export function isHttpsUrl(value: string): boolean {
  const trimmed = value.trim();
  if (!trimmed.startsWith(HTTPS_SCHEME_PREFIX)) return false;
  try {
    return new URL(trimmed).protocol === "https:";
  } catch {
    return false;
  }
}
