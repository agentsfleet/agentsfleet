// API URL normalisation — extracted from the deleted program/args.js so
// the helper lives near other URL/path utilities and survives the
// commander refactor.

export const DEFAULT_API_URL = "https://api.agentsfleet.net";
export const DEFAULT_DASHBOARD_URL = "https://app.agentsfleet.net";

const DEV_API_HOST = "api-dev.agentsfleet.net";
const PROD_API_HOST = "api.agentsfleet.net";
const DEV_DASHBOARD_URL = "https://app-dev.agentsfleet.net";

export function normalizeApiUrl(url: string | null | undefined): string {
  return String(url || DEFAULT_API_URL).replace(/\/+$/, "");
}

export function normalizeDashboardUrl(url: string | null | undefined): string {
  return String(url || DEFAULT_DASHBOARD_URL).replace(/\/+$/, "");
}

export function dashboardUrlForApiUrl(apiUrl: string): string {
  let host = "";
  try {
    host = new URL(normalizeApiUrl(apiUrl)).host;
  } catch {
    return DEFAULT_DASHBOARD_URL;
  }
  if (host === DEV_API_HOST) return DEV_DASHBOARD_URL;
  if (host === PROD_API_HOST) return DEFAULT_DASHBOARD_URL;
  return DEFAULT_DASHBOARD_URL;
}

export function resolveDashboardUrl(
  apiUrl: string,
  dashboardOverride: string | null | undefined,
): string {
  const override = dashboardOverride?.trim();
  return override ? normalizeDashboardUrl(override) : dashboardUrlForApiUrl(apiUrl);
}
