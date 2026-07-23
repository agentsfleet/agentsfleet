// Pure parsing and formatting for the Inspect dialog's request-context panel.
// Split out of `EventDetailsDialog` at its length cap — no JSX, no React, so
// the display components stay in the dialog and read these.

// A stored `request_json` is truncated before parse/render so a runaway
// webhook body cannot ride megabytes into the DOM.
export const REQUEST_CONTEXT_MAX_CHARS = 10_000;
export const REQUEST_CONTEXT_MAX_ENTRIES = 100;
export const COPIED_REQUEST_CONTEXT_OMITTED =
  "Omitted from copied diagnostic because webhook data may contain private or secret values.";

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

/** Parse the bounded request payload; a non-object or malformed body is kept
 * as its raw string rather than dropped, so the operator still sees what
 * arrived. */
export function parseRequestContext(raw: string): unknown {
  const request = raw.slice(0, REQUEST_CONTEXT_MAX_CHARS).trim();
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
  if (typeof value === "string") return value.slice(0, REQUEST_CONTEXT_MAX_CHARS);
  if (typeof value === "number") return String(value);
  return String(JSON.stringify(value)).slice(0, REQUEST_CONTEXT_MAX_CHARS);
}

export function previewRequestEntries(context: Record<string, unknown>): {
  entries: Array<[string, unknown]>;
  hasMore: boolean;
} {
  const entries = Object.entries(context);
  return {
    entries: entries.slice(0, REQUEST_CONTEXT_MAX_ENTRIES),
    hasMore: entries.length > REQUEST_CONTEXT_MAX_ENTRIES,
  };
}

/** The redaction stand-in copied into the diagnostic in place of the raw
 * payload, or null when there was nothing to omit. */
export function copiedRequestContext(raw: string): string | null {
  return raw.slice(0, REQUEST_CONTEXT_MAX_CHARS + 1).trim()
    ? COPIED_REQUEST_CONTEXT_OMITTED
    : null;
}
