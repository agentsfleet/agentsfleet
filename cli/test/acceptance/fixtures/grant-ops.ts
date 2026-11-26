/**
 * grant-ops.ts — a fleet's access request, as a terminal reads it.
 *
 * A fleet asks for a secret or an integration with `request_access`. The
 * request raises one card in `fleet approvals` and writes one grant in
 * `fleet grants`; the asking turn ends waiting, and a person's answer on the
 * card resumes the run in a turn sent by `continuation:<the turn that waited>`.
 */

import assert from "node:assert/strict";
import { Schema } from "effect";

import { GATE_STATUS } from "../../../src/constants/approvals.ts";
import type { PrimeSession } from "./prime-session.ts";
import { TURN_TIMEOUT_MS, TURN_WAITING, awaitTurn, processed, type Turn } from "./prime-turns.ts";

/** The gate kind an access request raises, as `afd_approval::KIND_INTEGRATION_GRANT` spells it. */
export const KIND_INTEGRATION_GRANT = "integration_grant";

/** Grant statuses, as `afd_api_wire::grant::status` spells them. */
export const GRANT_STATUS = {
  pending: "pending",
  approved: "approved",
  revoked: "revoked",
} as const;

/** How an approved grant reaches a run, as `afd_api_wire::grant::kind` spells it. */
export const GRANT_KIND = {
  secret: "secret",
  integration: "integration",
} as const;

/** The actor of the turn a person's answer writes (`afd_approval::inbox::resolve`). */
const CONTINUATION_ACTOR_PREFIX = "continuation:";
const CARD_POLL_MS = 2_000;
const APPROVALS = ["fleet", "approvals"] as const;
const FLEET_FLAG = "--fleet";

// `Grant` in rustd/crates/daemon/afd_api_wire/src/grant.rs.
const GrantSchema = Schema.Struct({
  id: Schema.String,
  kind: Schema.String,
  name: Schema.String,
  status: Schema.String,
  scope: Schema.NullOr(Schema.Unknown),
  approved_at: Schema.NullOr(Schema.String),
});
const GrantPageSchema = Schema.Struct({ items: Schema.Array(GrantSchema) });

// `ApprovalSummary` in rustd/crates/daemon/afd_api_wire/src/approval.rs. The
// evidence is what the request wrote: the name asked for, the kind and the scope.
const CardSchema = Schema.Struct({
  gate_id: Schema.String,
  fleet_id: Schema.String,
  gate_type: Schema.String,
  status: Schema.String,
  proposed_action: Schema.String,
  blast_radius: Schema.String,
  resolved_at: Schema.NullOr(Schema.String),
  resolved_by: Schema.String,
  evidence: Schema.NullOr(Schema.Struct({
    service: Schema.String,
    kind: Schema.String,
    scope: Schema.NullOr(Schema.Unknown),
  })),
});
const CardPageSchema = Schema.Struct({ items: Schema.Array(CardSchema) });
const DecisionSchema = Schema.Struct({
  gate_id: Schema.String,
  outcome: Schema.String,
  resolved_at: Schema.String,
  resolved_by: Schema.String,
});

export type Grant = typeof GrantSchema.Type;
export type Card = typeof CardSchema.Type;
export type Decision = typeof DecisionSchema.Type;

/** Every grant `fleet grants list` reports for the fleet. */
export async function grantsOf(session: PrimeSession, fleetId: string): Promise<ReadonlyArray<Grant>> {
  return Schema.decodeUnknownSync(GrantPageSchema)(await session.json(["fleet", "grants", "list", FLEET_FLAG, fleetId])).items;
}

/** The fleet's one grant for `name`; more than one row, or none, fails. */
export async function grantFor(session: PrimeSession, fleetId: string, name: string): Promise<Grant> {
  const grants = await grantsOf(session, fleetId);
  const mine = grants.filter((grant) => grant.name === name);
  assert.equal(mine.length, 1, `expected one grant for ${name}; fleet grants list shows ${JSON.stringify(grants)}`);
  return mine[0] as Grant;
}

/** The access cards `fleet approvals list --fleet` holds for the fleet, decided or not. */
export async function accessCards(session: PrimeSession, fleetId: string): Promise<ReadonlyArray<Card>> {
  const page = Schema.decodeUnknownSync(CardPageSchema)(await session.json([...APPROVALS, "list", FLEET_FLAG, fleetId]));
  assert.ok(page.items.every((card) => card.fleet_id === fleetId), `the filter let another fleet's card through: ${JSON.stringify(page)}`);
  return page.items.filter((card) => card.gate_type === KIND_INTEGRATION_GRANT);
}

/** One card, as `fleet approvals show` prints it. */
export async function showCard(session: PrimeSession, gateId: string): Promise<Card> {
  return Schema.decodeUnknownSync(CardSchema)(await session.json([...APPROVALS, "show", gateId]));
}

/** `fleet approvals approve|deny`, answering the decision the daemon recorded. */
export async function decide(session: PrimeSession, verb: "approve" | "deny", gateId: string): Promise<Decision> {
  return Schema.decodeUnknownSync(DecisionSchema)(await session.json([...APPROVALS, verb, gateId]));
}

export interface Asked {
  /** The one pending card the request raised. */
  readonly card: Card;
  /** The turn whose run asked, which ended waiting on the card. */
  readonly waiting: Turn;
}

/**
 * The one pending card the fleet's request raised, and the turn left waiting on
 * it. `sent` says what became of the message that asked, for the failure a
 * fleet that never asks ends in.
 */
export async function raisedCard(session: PrimeSession, fleetId: string, sent: Promise<string>): Promise<Asked> {
  const deadline = Date.now() + TURN_TIMEOUT_MS;
  let pending: ReadonlyArray<Card> = [];
  while (pending.length === 0 && Date.now() < deadline) {
    await Bun.sleep(CARD_POLL_MS);
    pending = (await accessCards(session, fleetId)).filter((card) => card.status === GATE_STATUS.pending);
  }
  if (pending.length === 0) {
    const answered = await Promise.race([sent, Bun.sleep(CARD_POLL_MS).then(() => "the send is still running")]);
    throw new Error(`fleet ${fleetId} raised no access card within ${TURN_TIMEOUT_MS}ms; ${answered}`);
  }
  assert.equal(pending.length, 1, `one request raises one card: ${JSON.stringify(pending)}`);
  const waiting = await awaitTurn(session, fleetId,
    (turns) => turns.find((turn) => turn.status === TURN_WAITING), "turn waiting on the card");
  return { card: pending[0] as Card, waiting };
}

/** The turn a person's answer wrote for the run `waiting` left, once that run has answered. */
export async function resumedTurn(session: PrimeSession, fleetId: string, waiting: Turn): Promise<Turn> {
  return awaitTurn(session, fleetId,
    processed((turn) => turn.actor === `${CONTINUATION_ACTOR_PREFIX}${waiting.turn_id}`), "answered turn resuming the request");
}
