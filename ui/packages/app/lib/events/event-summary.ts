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
// Re-exported below so every consumer keeps importing this one module.
import {
  eventHeadlineFrom,
  steerMessageFrom,
} from "./event-payload";

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
  GITHUB_APP: "GitHub App",
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
  if (base === ACTOR.CRON || base.startsWith(`${ACTOR.CRON}:`)) return SENDER.SCHEDULE;
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
  if (trimmed === "github" || trimmed === "github-app") return SENDER.GITHUB_APP;
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

export type EventFailurePresentation = {
  label: string;
  guidance: "startup" | null;
};

const FAILURE_PRESENTATION: Record<string, EventFailurePresentation> = {
  startup_posture: { label: "Failed a startup safety check", guidance: "startup" },
  budget_breach: { label: "Fleet budget limit reached", guidance: null },
  policy_deny: { label: "Blocked by fleet policy", guidance: null },
  timeout_kill: { label: "Timed out", guidance: null },
  oom_kill: { label: "Ran out of memory", guidance: null },
  resource_kill: { label: "Hit a resource limit", guidance: null },
  runner_crash: { label: "The runner crashed", guidance: null },
  transport_loss: { label: "Lost connection to the runner", guidance: null },
  landlock_deny: { label: "Blocked by the sandbox policy", guidance: null },
  lease_expired: { label: "The run's lease expired", guidance: null },
  renewal_terminate: { label: "Stopped by lease renewal policy", guidance: null },
};

export function failurePresentationFor(tag: string): EventFailurePresentation {
  return FAILURE_PRESENTATION[tag] ?? { label: tag, guidance: null };
}

export function failureSentenceFor(tag: string): string {
  return failurePresentationFor(tag).label;
}

/**
 * The remediation line shown under a failure sentence. Only classes the
 * operator can actually act on from the console carry one — a guidance line
 * that cannot be followed is noise, so an unmapped or unactionable class
 * returns null and renders nothing.
 */
export const GUIDANCE = {
  STARTUP: "Add this fleet's instructions on its Skill tab — the next delivery picks them up.",
} as const;

export function guidanceFor(tag: string | null | undefined): string | null {
  if (!tag) return null;
  return failurePresentationFor(tag).guidance === "startup" ? GUIDANCE.STARTUP : null;
}

// ── Outcome sentences ─────────────────────────────────────────────────────

export const OUTCOME = {
  WORKING: "Still working.",
  WAITING_APPROVAL: "Waiting for approval.",
  FAILED: "The run failed.",
  NO_REPLY: "Completed with no reply recorded.",
} as const;

const CAUSE_SEPARATOR = " — ";

/**
 * What to say about an event that recorded no reply. Never empty — this is the
 * floor that guarantees no rendered row is blank. A failure with a recorded
 * cause line renders it after the plain-language sentence, so the operator
 * reads WHICH check failed, not only that one did.
 */
export function outcomeFor(
  row: Pick<EventRow, "status" | "failure_label"> & Partial<Pick<EventRow, "failure_detail">>,
): string {
  if (row.status === EVENT_STATUS.RECEIVED) return OUTCOME.WORKING;
  if (row.status === EVENT_STATUS.GATE_BLOCKED) return OUTCOME.WAITING_APPROVAL;
  if (row.failure_label) {
    const sentence = failureSentenceFor(row.failure_label);
    const detail = (row.failure_detail ?? "").trim();
    // A detail that merely restates the sentence would read twice; only a
    // distinct cause earns the second clause.
    if (detail.length > 0 && detail !== sentence) return `${sentence}${CAUSE_SEPARATOR}${detail}`;
    return sentence;
  }
  if (row.status === EVENT_STATUS.FLEET_ERROR) return OUTCOME.FAILED;
  return OUTCOME.NO_REPLY;
}

/** The same floor for a live frame, which carries a status but no durable row. */
export function outcomeForStatus(status: EventStatusValue): string {
  return outcomeFor({ status, failure_label: null });
}

/**
 * The floor for a live completion frame, which may carry the failure cause the
 * durable row will hold — so the chat shows the real sentence without reload.
 * Empty strings mean "no cause" (the publisher's wire convention).
 */
export function outcomeForCompletion(
  status: EventStatusValue,
  failureLabel: string | undefined,
  failureDetail: string | undefined,
): string {
  const label = (failureLabel ?? "").trim();
  return outcomeFor({
    status,
    failure_label: label.length > 0 ? label : null,
    failure_detail: failureDetail ?? null,
  });
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

// The payload vocabulary is part of this module's public surface — split for
// file length only. Importing from `event-payload` directly is equally valid;
// these keep every existing call site working unchanged.
export {
  eventHeadlineFrom,
  eventLinkFrom,
  eventReferenceFrom,
  HEADLINE,
  parsePayload,
  steerMessageFrom,
} from "./event-payload";
