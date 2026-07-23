// What a stored `request_json` payload says — parsing, headlines, and the
// source reference and outbound link a recognised shape earns.
//
// Split out of `event-summary.ts` at its length cap, NOT forked from it: this
// is the same single vocabulary, and `event-summary` re-exports every name
// here so callers keep one import and the two files can never drift into
// per-surface copies — one voice, three surfaces.

// ── Event headlines ───────────────────────────────────────────────────────

export const HEADLINE = {
  RECEIVED_SUFFIX: "received",
  EVENT_FALLBACK: "Event received",
} as const;

const PAYLOAD_MESSAGE_KEY = "message";
const SEPARATOR = " · ";
const TITLE_SEPARATOR = " — ";

type Payload = Record<string, unknown>;

/** Parse a stored request payload, tolerating absence and malformed text. */
export function parsePayload(requestJson: string | null | undefined): Payload | null {
  if (!requestJson) return null;
  try {
    const parsed: unknown = JSON.parse(requestJson);
    if (!parsed || typeof parsed !== "object" || Array.isArray(parsed)) return null;
    return parsed as Payload;
  } catch {
    return null;
  }
}

function text(payload: Payload, key: string): string {
  const value = payload[key];
  return typeof value === "string" ? value.trim() : "";
}

function count(payload: Payload, key: string): number | null {
  const value = payload[key];
  return typeof value === "number" && Number.isFinite(value) ? value : null;
}

/**
 * The operator's own submitted message, which lives in the stored request
 * payload — the reply field belongs to the fleet, not to the operator.
 */
export function steerMessageFrom(requestJson: string | null | undefined): string {
  const payload = parsePayload(requestJson);
  return payload ? text(payload, PAYLOAD_MESSAGE_KEY) : "";
}

/**
 * A one-line headline for an event that arrived from an integration, built
 * only from fields the normalized payload actually carries. Two shapes are
 * recognised (a change proposal and a completed run); anything else names the
 * event rather than pretending to summarise it.
 */
export function eventHeadlineFrom(
  requestJson: string | null | undefined,
  eventType: string,
): string {
  const payload = parsePayload(requestJson);
  if (!payload) return neutralHeadline(eventType);
  return (
    changeProposalHeadline(payload) ?? completedRunHeadline(payload) ?? neutralHeadline(eventType)
  );
}

function neutralHeadline(eventType: string): string {
  const kind = eventType.trim();
  if (kind.length === 0) return HEADLINE.EVENT_FALLBACK;
  return `${kind} ${HEADLINE.RECEIVED_SUFFIX}`;
}

// `{action, repo, number, title, …}` — the normalized change-proposal shape.
function changeProposalHeadline(payload: Payload): string | null {
  const repo = text(payload, "repo");
  const number = count(payload, "number");
  if (repo.length === 0 || number === null) return null;
  const action = text(payload, "action");
  const subject = action.length > 0 ? `${action}${SEPARATOR}` : "";
  const title = text(payload, "title");
  const suffix = title.length > 0 ? `${TITLE_SEPARATOR}${title}` : "";
  return `${subject}${repo}#${number}${suffix}`;
}

// `{workflow_name, conclusion, repo, head_branch, …}` — the completed-run shape.
function completedRunHeadline(payload: Payload): string | null {
  const name = text(payload, "workflow_name");
  const conclusion = text(payload, "conclusion");
  if (name.length === 0 || conclusion.length === 0) return null;
  const repo = text(payload, "repo");
  const branch = text(payload, "head_branch");
  const where = [repo, branch].filter((part) => part.length > 0).join(SEPARATOR);
  return where.length > 0 ? `${name} ${conclusion}${SEPARATOR}${where}` : `${name} ${conclusion}`;
}

// ── Source reference and link ─────────────────────────────────────────────
// The two recognised payload shapes carry their own canonical link
// (`url` on a change proposal, `run_url` on a completed run — both are the
// provider's `html_url`). A shape without one renders no link rather than a
// guessed one.

const PAYLOAD_LINK_KEYS = ["url", "run_url"] as const;
// Webhook payloads are third-party input. Only these two schemes may ever
// reach an href — `javascript:` and `data:` in a rendered link are script
// execution, so the allowlist is the guard, not a blocklist of known-bad.
const SAFE_LINK_SCHEMES = ["https:", "http:"] as const;

/**
 * The source reference a recognised change proposal carries. It is shown
 * once as the outbound link label, so the event headline can stay readable
 * instead of repeating the same repository and number on the next line.
 */
export function eventReferenceFrom(requestJson: string | null | undefined): string | null {
  const payload = parsePayload(requestJson);
  if (!payload) return null;
  const repo = text(payload, "repo");
  const number = count(payload, "number");
  return repo.length > 0 && number !== null ? `${repo}#${number}` : null;
}

/**
 * The outbound link a recognised payload carries, or null. Anything that is
 * not an absolute http(s) URL returns null — including a relative path, which
 * would resolve against the console's own origin and send the operator
 * somewhere this fleet never named.
 */
export function eventLinkFrom(requestJson: string | null | undefined): string | null {
  const payload = parsePayload(requestJson);
  if (!payload) return null;
  for (const key of PAYLOAD_LINK_KEYS) {
    const raw = text(payload, key);
    if (raw.length === 0) continue;
    let parsed: URL;
    try {
      parsed = new URL(raw);
    } catch {
      continue;
    }
    if (SAFE_LINK_SCHEMES.some((scheme) => parsed.protocol === scheme)) return parsed.href;
  }
  return null;
}
