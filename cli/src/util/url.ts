// API URL normalisation — extracted from the deleted program/args.js so
// the helper lives near other URL/path utilities and survives the
// commander refactor.

export const DEFAULT_API_URL = "https://api.agentsfleet.net";

export function normalizeApiUrl(url: string | null | undefined): string {
  return String(url || DEFAULT_API_URL).replace(/\/+$/, "");
}
