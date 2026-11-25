// What a journey reads back once its fleet exists: the fleet's turns, as
// `fleet turns --json` lists them, one turn's stored request and reply, and a
// message sent with `fleet steer`.
//
// A journey asserts what a turn did (who sent it, how it ended), and reads the
// model's words only to find a word or number this run made up.

import assert from "node:assert/strict";
import crypto from "node:crypto";
import { Schema } from "effect";

import { TURN_STATUS } from "../../../src/constants/turn-status.ts";
import { nativeRequest, readAuthContext } from "./native-api.ts";
import type { PrimeSession } from "./prime-session.ts";
import { trailingJson } from "./steer-envelope.ts";

// `TurnSummary` in rustd/crates/protocol/af_wire/src/turn/mod.rs, as far as a journey reads it.
const TurnSchema = Schema.Struct({
  turn_id: Schema.String,
  actor: Schema.String,
  event_type: Schema.String,
  status: Schema.String,
  failure_label: Schema.NullOr(Schema.String),
  failure_detail: Schema.NullOr(Schema.String),
});
const TurnPageSchema = Schema.Struct({ items: Schema.Array(TurnSchema) });
// `TurnDetail` in the same file, as far as a journey reads it.
const TurnDetailSchema = Schema.Struct({
  turn_id: Schema.String,
  request_json: Schema.String,
  response_text: Schema.NullOr(Schema.String),
});
const SteeredSchema = Schema.Struct({ turn_id: Schema.String, kind: Schema.String, status: Schema.String });

export type Turn = typeof TurnSchema.Type;
export type TurnDetail = typeof TurnDetailSchema.Type;
/** Picks a turn from the fleet's turns, or none yet; it may read more of each turn before it answers. */
export type FindTurn = (turns: ReadonlyArray<Turn>) => Turn | undefined | Promise<Turn | undefined>;

/** A turn whose run asked a person and ended there (`af_core::turn::status::WAITING`). */
export const TURN_WAITING = "waiting";
// `af_core::turn::status::CANCELLED`: a person stopped the turn.
const TURN_CANCELLED = "cancelled";
const ENDED_UNANSWERED: ReadonlyArray<string> = [TURN_STATUS.FLEET_ERROR, TURN_STATUS.GATE_BLOCKED, TURN_CANCELLED];

const TURN_POLL_MS = 2_000;
// The runner queue, one provider round trip, and the report: the budget every live run gets.
export const TURN_TIMEOUT_MS = 180_000;
// A steer waits for its turn to end, so its child outlives the turn's budget.
export const STEER_TIMEOUT_MS = 240_000;

const STEER = ["fleet", "steer"] as const;
const KIND_COMPLETE = "complete";
// `fleet steer` prints the answer under this line (src/commands/fleet/steer-turns.ts).
const FINAL_REPLY = "Final reply:";
const PROCESSED_LINE = /\bturn [\da-f-]+ processed/u;
const TOKEN_BYTES = 4;

/** A word nothing but this run's message carries, so finding it means this run's turn. */
export function runWord(stem: string): string {
  return `${stem}${crypto.randomBytes(TOKEN_BYTES).toString("hex")}`;
}

/** The fleet's turns, newest first. */
export async function turnsOf(session: PrimeSession, fleetId: string): Promise<ReadonlyArray<Turn>> {
  return Schema.decodeUnknownSync(TurnPageSchema)(await session.json(["fleet", "turns", fleetId])).items;
}

/**
 * One turn's stored request and whole reply. `fleet turns` lists summaries
 * that carry neither, and no command prints a turn the person did not send,
 * so this reads the turn from the API with the CLI's own credential and
 * workspace.
 */
export async function turnDetail(session: PrimeSession, fleetId: string, turnId: string): Promise<TurnDetail> {
  const context = await readAuthContext(session.env());
  const path = `/fleets/${encodeURIComponent(fleetId)}/turns/${encodeURIComponent(turnId)}`;
  return Schema.decodeUnknownSync(TurnDetailSchema)(await nativeRequest(context, path));
}

/**
 * Finds the processed turn among those `pick` chooses. A chosen turn that
 * failed, was refused or was stopped fails the journey with its cause, so a
 * run that died is not waited out.
 */
export function processed(pick: (turn: Turn) => boolean): FindTurn {
  return (turns) => {
    const chosen = turns.filter(pick);
    const unanswered = chosen.find((turn) => ENDED_UNANSWERED.includes(turn.status));
    if (unanswered !== undefined) {
      throw new Error(`turn ${unanswered.turn_id} ended ${unanswered.status}: ` +
        `${unanswered.failure_label ?? "no label"}, ${unanswered.failure_detail ?? "no detail"}`);
    }
    return chosen.find((turn) => turn.status === TURN_STATUS.PROCESSED);
  };
}

/** Reads the fleet's turns until `find` answers one; `what` names it when none comes in time. */
export async function awaitTurn(
  session: PrimeSession, fleetId: string, find: FindTurn, what: string, timeoutMs: number = TURN_TIMEOUT_MS,
): Promise<Turn> {
  const deadline = Date.now() + timeoutMs;
  let turns: ReadonlyArray<Turn> = [];
  for (;;) {
    turns = await turnsOf(session, fleetId);
    const found = await find(turns);
    if (found !== undefined) return found;
    if (Date.now() >= deadline) break;
    await Bun.sleep(TURN_POLL_MS);
  }
  throw new Error(`fleet ${fleetId} showed no ${what} within ${timeoutMs}ms; its turns: ${JSON.stringify(turns)}`);
}

/** One message through `fleet steer --json`: the id of its turn, which must have ended processed. */
export async function steer(
  session: PrimeSession, fleet: string, message: string, timeoutMs: number = STEER_TIMEOUT_MS,
): Promise<string> {
  const result = await session.run([...STEER, fleet, message, "--json"], timeoutMs);
  assert.equal(result.code, 0, `fleet steer exited ${result.code}: stdout=${result.stdout} stderr=${result.stderr}`);
  const steered = Schema.decodeUnknownSync(SteeredSchema)(trailingJson(result.stdout));
  assert.equal(steered.kind, KIND_COMPLETE, `the steer did not wait out its turn: ${JSON.stringify(steered)}`);
  assert.equal(steered.status, TURN_STATUS.PROCESSED, `the turn was not answered: ${JSON.stringify(steered)}`);
  return steered.turn_id;
}

/** One message through `fleet steer` as a person reads it: the fleet's final reply. */
export async function steerForReply(session: PrimeSession, fleet: string, message: string): Promise<string> {
  const result = await session.run([...STEER, fleet, message], STEER_TIMEOUT_MS);
  assert.equal(result.code, 0, `fleet steer exited ${result.code}: stdout=${result.stdout} stderr=${result.stderr}`);
  const at = result.stdout.lastIndexOf(FINAL_REPLY);
  assert.ok(at >= 0, `the terminal showed no final reply: ${result.stdout}`);
  const reply = result.stdout.slice(at + FINAL_REPLY.length);
  assert.match(reply, PROCESSED_LINE, `the terminal did not confirm the answered turn: ${result.stdout}`);
  // Only what follows the final reply: a tool the run called may answer with turn ids of its own above it.
  assert.doesNotMatch(reply, /"turn_id"\s*:/u, "a conversation a person reads must not print the machine envelope");
  return reply;
}
