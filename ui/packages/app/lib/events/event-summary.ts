// Operator-readable text for a durable event row.
//
// Every sentence produced here is derived from a field the row already carries.
// Nothing is invented: an event with no recorded reply says so, an unmapped
// runner failure renders its own tag rather than a guess, and a payload shape
// this module does not recognise falls back to naming what arrived instead of
// rendering an empty row.
//
// Three surfaces read this module — the console thread, the console summary
// strip, and the events table — so the vocabulary cannot drift between them.

import type { EventRow, EventStatusValue } from "@/lib/api/events";

// ── Actor vocabulary ──────────────────────────────────────────────────────
// Mirrors what the server writes: `steer:<user_id>` / `steer:api`
// (fleets/messages.zig buildSteerActor), `webhook:<source>`
// (webhooks/fleet.zig), a platform identity such as `github-app`
// (fleet_runtime/webhook_verify.zig), and the runtime's own actors.

export const ACTOR = {
  STEER_PREFIX: "steer:",
  WEBHOOK_PREFIX: "webhook:",
  API_STEER: "steer:api",
  FLEET: "fleet",
  CRON: "cron",
  CONTINUATION: "continuation",
  CONFIG_RELOAD: "config_reload",
  GATE_BLOCKED: "gate_blocked",
} as const;

export const EVENT_STATUS = {
  RECEIVED: "received",
  PROCESSED: "processed",
  FLEET_ERROR: "fleet_error",
  GATE_BLOCKED: "gate_blocked",
} as const;

// ── Sender labels ─────────────────────────────────────────────────────────
// The actor field carries an opaque account identifier for a steer, which no
// operator can read. The rendered sender is a word, never an identifier.

export const SENDER = {
  OPERATOR: "Operator",
  API: "API",
  FLEET_FALLBACK: "Fleet",
  SCHEDULE: "Schedule",
  CONTINUATION: "Continuation",
  CONFIG_RELOAD: "Config reload",
  APPROVAL_GATE: "Approval gate",
  UNKNOWN: "System",
} as const;

export type MessageRole = "user" | "assistant" | "system";

/** Which side of the conversation an actor sits on. */
export function roleFor(actor: string): MessageRole {
  // A continuation carries the trigger it resumed; a resumed steer is still the
  // operator's turn.
  const base = actor.startsWith("continuation:") ? actor.slice("continuation:".length) : actor;
  if (base.startsWith(ACTOR.STEER_PREFIX)) return "user";
  if (base === ACTOR.FLEET) return "assistant";
  return "system";
}

// A source segment that is a bare opaque identifier — a Clerk user id, a Slack
// member id, a UUID — must never render. These reach the label through a
// `steer:<uid>` an exact match missed, a `continuation:steer:<uid>` chain, or a
// connector actor like `slack:<member-id>`. The whole point of the sender label
// is that no operator sees an identifier (Invariant 2).
const OPAQUE_ID = /^(user_|sess_|org_)/i;
const CONTINUATION_PREFIX = "continuation:";

/**
 * The name rendered beside a message. `fleetName` is the console's own fleet —
 * the design labels the fleet's messages with the fleet's name, not the word
 * "fleet". An actor this vocabulary does not recognise renders a readable slug
 * (`github-app`), never a raw account or member identifier.
 */
export function senderLabelFor(actor: string, fleetName?: string): string {
  // A continuation of a steer is still that operator speaking, not a new actor.
  const base = actor.startsWith(CONTINUATION_PREFIX)
    ? actor.slice(CONTINUATION_PREFIX.length)
    : actor;
  if (base === ACTOR.API_STEER) return SENDER.API;
  if (base.startsWith(ACTOR.STEER_PREFIX)) return SENDER.OPERATOR;
  if (base === ACTOR.FLEET) return fleetName && fleetName.length > 0 ? fleetName : SENDER.FLEET_FALLBACK;
  if (base.startsWith(ACTOR.WEBHOOK_PREFIX)) return sourceOf(base.slice(ACTOR.WEBHOOK_PREFIX.length));
  if (base === ACTOR.CRON) return SENDER.SCHEDULE;
  if (base === ACTOR.CONTINUATION) return SENDER.CONTINUATION;
  if (base === ACTOR.CONFIG_RELOAD) return SENDER.CONFIG_RELOAD;
  if (base === ACTOR.GATE_BLOCKED) return SENDER.APPROVAL_GATE;
  // A connector actor such as `slack:<member-id>`: name the source, drop the id.
  const colon = base.indexOf(":");
  if (colon > 0) return sourceOf(base.slice(0, colon));
  return sourceOf(base);
}

// A single source token: safe to render if it isn't an opaque identifier.
function sourceOf(token: string): string {
  const trimmed = token.trim();
  if (trimmed.length === 0 || OPAQUE_ID.test(trimmed)) return SENDER.UNKNOWN;
  return trimmed;
}

/** Two letters for the sender chip — initials, never an identifier fragment. */
export function senderInitialsFor(label: string): string {
  const words = label.split(/[\s·\-_/]+/).filter((word) => word.length > 0);
  const first = words[0] ?? "";
  const second = words[1] ?? "";
  // `charAt` is total — empty words are already filtered out, so there is no
  // missing-character case to defend against.
  const initials =
    second.length > 0 ? `${first.charAt(0)}${second.charAt(0)}` : first.slice(0, 2);
  return initials.toUpperCase();
}

// ── Runner failure vocabulary ─────────────────────────────────────────────
// The runner's FailureClass tags (src/lib/contract/execution_result.zig) in
// plain English. A tag this list has not caught up to renders its own name
// rather than throwing or hiding the failure.

const FAILURE_SENTENCE: Record<string, string> = {
  startup_posture: "Failed a startup safety check",
  policy_deny: "Blocked by fleet policy",
  timeout_kill: "Timed out",
  oom_kill: "Ran out of memory",
  resource_kill: "Hit a resource limit",
  runner_crash: "The runner crashed",
  transport_loss: "Lost connection to the runner",
  landlock_deny: "Blocked by the sandbox policy",
  lease_expired: "The run's lease expired",
  renewal_terminate: "Stopped by lease renewal policy",
};

export function failureSentenceFor(tag: string): string {
  return FAILURE_SENTENCE[tag] ?? tag;
}

// ── Outcome sentences ─────────────────────────────────────────────────────

export const OUTCOME = {
  WORKING: "Still working.",
  WAITING_APPROVAL: "Waiting for approval.",
  FAILED: "The run failed.",
  NO_REPLY: "Completed with no reply recorded.",
} as const;

/**
 * What to say about an event that recorded no reply. Never empty — this is the
 * floor that guarantees no rendered row is blank.
 */
export function outcomeFor(row: Pick<EventRow, "status" | "failure_label">): string {
  if (row.status === EVENT_STATUS.RECEIVED) return OUTCOME.WORKING;
  if (row.status === EVENT_STATUS.GATE_BLOCKED) return OUTCOME.WAITING_APPROVAL;
  if (row.failure_label) return failureSentenceFor(row.failure_label);
  if (row.status === EVENT_STATUS.FLEET_ERROR) return OUTCOME.FAILED;
  return OUTCOME.NO_REPLY;
}

/** The same floor for a live frame, which carries a status but no durable row. */
export function outcomeForStatus(status: EventStatusValue): string {
  return outcomeFor({ status, failure_label: null });
}

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

/**
 * The trigger a durable row renders — the thing that WOKE the fleet, never the
 * fleet's answer. A durable event row is one conversation turn: the actor and
 * `request_json` name the trigger (an operator's steer, a webhook, a cron),
 * and `response_text` carries the fleet's reply written back onto the SAME row
 * (event_rows.zig UPDATEs response_text WHERE event_id). So the trigger body
 * comes only from the actor + request payload; the reply is `replyBodyFor`.
 *
 * An assistant-actor row has no separate trigger (it IS a reply); returns "".
 */
export function triggerBodyFor(row: Pick<EventRow, "actor" | "request_json" | "event_type">): string {
  const role = roleFor(row.actor);
  if (role === "user") return steerMessageFrom(row.request_json);
  if (role === "assistant") return "";
  return eventHeadlineFrom(row.request_json, row.event_type);
}

/**
 * The fleet's reply on a durable row — the text UPDATEd into `response_text`
 * after the trigger's turn completes. Empty until the fleet answers; the row
 * conversion falls back to `outcomeFor` so a reply-less turn still says what
 * happened rather than rendering blank.
 */
export function replyBodyFor(row: Pick<EventRow, "response_text">): string {
  return (row.response_text ?? "").trim();
}
