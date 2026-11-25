// The first-message journey, from a terminal: one message creates
// morning-rooster, its first turn is answered, and a later message is answered
// in the same chat.
//
// The fleet's name, its first message and what a person types next are
// `tests/fixtures/fleets/morning-rooster.json`. Each message carries a word
// this run made up, so a reply holding it answers this run's message.

import { describe, it } from "bun:test";
import assert from "node:assert/strict";
import { Schema } from "effect";

import { ACCEPTANCE_TARGET_ENV, AGENTSFLEET_STATUS } from "./fixtures/constants.ts";
import { getStatus } from "./fixtures/lifecycle.ts";
import { refusalCode } from "./fixtures/negatives.ts";
import { entry, primeFleet as primeFixture, said } from "./fixtures/prime-fleets.ts";
import { fleetsNamed, isLive, ownWorkspace, primeFleet, primeSession, removeFleetsNamed } from "./fixtures/prime-session.ts";
import { awaitTurn, processed, runWord, steer, steerForReply, turnsOf } from "./fixtures/prime-turns.ts";
import { REJECTION_CODE } from "../../src/constants/rejection.ts";
import { EXIT_CODE } from "../../src/errors/index.ts";

const fixture = primeFixture("morning-rooster");
const followUp = entry(fixture.follow_ups, 0, "follow-up");
const WORKSPACE = `fixture-cli-${fixture.journey}`;

// The fleet list, the first turn's run and two answered messages.
const JOURNEY_TIMEOUT_MS = 300_000;
// A model reference no workspace can use: the create is refused and writes nothing.
const NONEXISTENT_MODEL = "accounts/fireworks/models/kimi-k2.6";
// af_core/src/error_code: the workspace holds no model a fleet could run on.
const CODE_NO_USABLE_MODEL = "UZ-SETUP-008";
const WHITESPACE_MESSAGE = "   ";
// `afd_turns::view::steer::ACTOR_PREFIX`: a person's message, as a turn records its sender.
const PERSON_ACTOR_PREFIX = "steer:";
// `af_wire::turn::TurnKind::Chat`: a message, as a turn records how it arrived.
const TURN_KIND_CHAT = "chat";
const UUID = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/u;

const FleetListSchema = Schema.Struct({
  items: Schema.Array(Schema.Struct({ id: Schema.String, name: Schema.String, status: Schema.String })),
});

/** The follow-up, asking for this run's word at the end of the reply. */
const askFor = (word: string): string => said(followUp, `End your reply with the word ${word}.`);

if (!isLive) {
  describe("prime-morning-rooster-live.spec.ts", () => {
    it.skip(`requires ${ACCEPTANCE_TARGET_ENV} to be an https URL`, () => {});
  });
} else {
  describe("morning-rooster: one message creates a fleet that answers in its own chat", () => {
    const session = primeSession(ownWorkspace(WORKSPACE));
    let fleetId = "";

    it("a create on a model the workspace cannot use is refused and writes no fleet", async () => {
      await removeFleetsNamed(session, fixture.name);
      const refused = await session.run(
        ["--json", "fleet", "create", fixture.first_message, "--name", fixture.name, "--model", NONEXISTENT_MODEL]);
      assert.equal(refused.code, EXIT_CODE.ServerError, `the create was not refused: ${refused.stdout}${refused.stderr}`);
      assert.equal(refusalCode(refused), CODE_NO_USABLE_MODEL);
      assert.equal(refused.stdout.trim(), "", "a refused create must print no fleet");
      assert.deepEqual(await fleetsNamed(session, fixture.name), [], "a refused create must write no fleet");
    });

    it("fleet create prints the fleet and its first turn, and that turn is answered", async () => {
      const created = await primeFleet(session, fixture);
      assert.match(created.fleet_id, UUID, `fleet create named no fleet: ${JSON.stringify(created)}`);
      assert.match(created.turn_id, UUID, `fleet create named no first turn: ${JSON.stringify(created)}`);
      fleetId = created.fleet_id;

      const first = await awaitTurn(session, fleetId,
        processed((turn) => turn.turn_id === created.turn_id), "answered first turn");
      assert.equal(first.event_type, TURN_KIND_CHAT);
      assert.ok(first.actor.startsWith(PERSON_ACTOR_PREFIX), `the first turn's sender is not a person: ${first.actor}`);
    }, JOURNEY_TIMEOUT_MS);

    it("fleet list and fleet show name it, active", async () => {
      assert.ok(fleetId, "the fleet was not created");
      const listed = Schema.decodeUnknownSync(FleetListSchema)(await session.json(["fleet", "list"])).items
        .filter((fleet) => fleet.id === fleetId);
      assert.deepEqual(listed, [{ id: fleetId, name: fixture.name, status: AGENTSFLEET_STATUS.active }]);

      const shown = await getStatus(session.env(), fleetId);
      assert.equal(shown.name, fixture.name);
      assert.equal(shown.status, AGENTSFLEET_STATUS.active);
    });

    it("fleet steer --json waits out the answer to a later message, in the same chat", async () => {
      assert.ok(fleetId, "the fleet was not created");
      const turnId = await steer(session, fleetId, askFor(runWord("rooster")));
      const turns = await turnsOf(session, fleetId);
      const steered = turns.find((turn) => turn.turn_id === turnId);
      assert.ok(steered, `the fleet's chat does not hold the steered turn ${turnId}: ${JSON.stringify(turns)}`);
      assert.ok(steered.actor.startsWith(PERSON_ACTOR_PREFIX), `the turn's sender is not a person: ${steered.actor}`);
      assert.equal(turns.filter((turn) => turn.event_type === TURN_KIND_CHAT).length, 2,
        `one chat holds the first message and the later one: ${JSON.stringify(turns)}`);
    }, JOURNEY_TIMEOUT_MS);

    it("fleet steer @name shows a person the fleet's reply to their message", async () => {
      assert.ok(fleetId, "the fleet was not created");
      const word = runWord("rooster");
      const reply = await steerForReply(session, `@${fixture.name}`, askFor(word));
      assert.ok(reply.includes(word), `the reply does not answer this run's message (${word}): ${reply}`);
    }, JOURNEY_TIMEOUT_MS);

    it("a message of spaces is refused before any turn is written", async () => {
      assert.ok(fleetId, "the fleet was not created");
      const before = (await turnsOf(session, fleetId)).length;
      const refused = await session.run(["fleet", "steer", fleetId, WHITESPACE_MESSAGE, "--json"]);
      assert.equal(refused.code, EXIT_CODE.ValidationError, `spaces were sent as a message: ${refused.stdout}`);
      assert.equal(refusalCode(refused), REJECTION_CODE.invalidArgument);
      assert.match(refused.stderr, /message is required/iu);
      assert.equal((await turnsOf(session, fleetId)).length, before, "a refused message must write no turn");
    });
  });
}
