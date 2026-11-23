// Pure parsing and formatting for the Inspect dialog's request-context panel.
// Split out of `TurnDetailsDialog` at its length cap — no JSX, no React, so
// the display components stay in the dialog and read these.

const REQUEST_CONTEXT_LABELS: Record<string, string> = {
  action: "Action",
  author: "Author",
  base_ref: "Base branch",
  draft: "Draft",
  head_ref: "Head branch",
  head_sha: "Head commit",
  number: "Number",
  pull_request: "Pull request",
  received_at: "Received",
  repo: "Repository",
  state: "State",
  title: "Title",
};

/** Parse the recorded request payload; a non-object or malformed body is kept
 * as its raw string rather than dropped, so the operator still sees what
 * arrived. */
export function parseRequestContext(raw: string): unknown {
  const request = raw.trim();
  if (!request) return null;
  try {
    return JSON.parse(request) as unknown;
  } catch {
    return request;
  }
}

export function isRequestContextRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

export function presentRequestLabel(key: string, githubSource: boolean): string {
  if (key === "url") return githubSource ? "Pull request" : "URL";
  return REQUEST_CONTEXT_LABELS[key] ?? key.replaceAll("_", " ");
}

export function formatRequestValue(value: unknown): string {
  if (value === null) return "—";
  if (typeof value === "boolean") return value ? "Yes" : "No";
  if (typeof value === "string") return value;
  if (typeof value === "number") return String(value);
  return String(JSON.stringify(value));
}
