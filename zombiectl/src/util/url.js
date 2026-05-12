// API URL normalisation — extracted from the deleted program/args.js so
// the helper lives near other URL/path utilities and survives the
// commander refactor.

export const DEFAULT_API_URL = "https://api.usezombie.com";

export function normalizeApiUrl(url) {
  return String(url || DEFAULT_API_URL).replace(/\/+$/, "");
}
