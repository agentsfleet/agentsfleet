// The fleet-to-fleet journey, from a terminal: queen-bee steers busy-bee, and
// reports what came back.
//
// Both fleets, their messages and which one the other steers are
// `tests/fixtures/fleets/queen-bee.json` and `busy-bee.json`. Only busy-bee is
// given this run's word, so queen-bee can report it only by asking.

import { describe, it, beforeAll } from "bun:test";
import assert from "node:assert/strict";

import { ACCEPTANCE_TARGET_ENV } from "./fixtures/constants.ts";
import { refusalCode } from "./fixtures/negatives.ts";
import { entry, primeFleet as primeFixture, said } from "./fixtures/prime-fleets.ts";
import { isLive, ownWorkspace, primeFleet, primeSession } from "./fixtures/prime-session.ts";
import { awaitTurn, processed, runWord, steer, steerForReply, turnsOf } from "./fixtures/prime-turns.ts";
import { REJECTION_CODE } from "../../src/constants/rejection.ts";
import { EXIT_CODE } from "../../src/errors/index.ts";

const queen = primeFixture("queen-bee");
const busy = primeFixture(entry(queen.steers, 0, "steered fleet"));
const ask = entry(queen.follow_ups, 0, "follow-up");
const WORKSPACE = `fixture-cli-${queen.journey}`;

// Two creates and their first turns, then two messages that each start a run on both fleets.
const SETUP_TIMEOUT_MS = 720_000;
const STEP_TIMEOUT_MS = 480_000;
// `afd_turns::view::steer`: a turn names its sender, a fleet as `fleet:<id>` and a person as `steer:<subject>`.
const FLEET_ACTOR_PREFIX = "fleet:";
const PERSON_ACTOR_PREFIX = "steer:";

if (!isLive) {
  describe("prime-queen-bee-live.spec.ts", () => {
    it.skip(`requires ${ACCEPTANCE_TARGET_ENV} to be an https URL`, () => {});
  });
} else {
  describe("queen-bee and busy-bee: one fleet steers another and reports its answer", () => {
    const session = primeSession(ownWorkspace(WORKSPACE));
    const word = runWord("honey");
    let queenId = "";
    let busyId = "";

    /** The follow-up, naming the fleet to steer by the id `fleet_steer` takes. */
    const askQueen = (added = ""): string => said(ask, `${busy.name}'s fleet_id is ${busyId}.${added}`);

    beforeAll(async () => {
      const worker = await primeFleet(session, busy, `Today's word is ${word}.`);
      busyId = worker.fleet_id;
      const director = await primeFleet(session, queen);
      queenId = director.fleet_id;
      await awaitTurn(session, busyId, processed((turn) => turn.turn_id === worker.turn_id), "answered first turn");
      await awaitTurn(session, queenId, processed((turn) => turn.turn_id === director.turn_id), "answered first turn");
    }, SETUP_TIMEOUT_MS);

    it("queen-bee's fleet_steer lands in busy-bee's chat as a turn the fleet sent, and is answered", async () => {
      assert.ok(queenId && busyId, "the two fleets were not created");
      await steer(session, queenId, askQueen());

      const asked = await awaitTurn(session, busyId,
        processed((turn) => turn.actor === `${FLEET_ACTOR_PREFIX}${queenId}`), "answered turn sent by queen-bee");
      const turns = await turnsOf(session, busyId);
      const people = turns.filter((turn) => turn.actor.startsWith(PERSON_ACTOR_PREFIX));
      assert.equal(people.length, 1,
        `a person sent busy-bee its first message only; the request (${asked.turn_id}) is the fleet's: ${JSON.stringify(turns)}`);
    }, STEP_TIMEOUT_MS);

    it("queen-bee reports the word only busy-bee was given", async () => {
      assert.ok(queenId && busyId, "the two fleets were not created");
      // busy-bee has answered by now, so its chat holds the word whichever
      // order queen-bee's run asks and reads in.
      const reply = await steerForReply(session, `@${queen.name}`,
        askQueen(" It has answered this before: read its chat with fleet_activity and repeat the word it said."));
      assert.ok(reply.includes(word), `queen-bee did not report busy-bee's word (${word}): ${reply}`);
    }, STEP_TIMEOUT_MS);

    it("a fleet no fixture names cannot be steered by name", async () => {
      const stranger = `${busy.name}-drone`;
      const refused = await session.run(["fleet", "steer", `@${stranger}`, ask, "--json"]);
      assert.equal(refused.code, EXIT_CODE.ValidationError, `a fleet that does not exist was steered: ${refused.stdout}`);
      assert.equal(refusalCode(refused), REJECTION_CODE.invalidArgument);
      assert.ok(refused.stderr.includes(`No fleet named \`${stranger}\` in this workspace`),
        `the refusal does not name the fleet it could not find: ${refused.stderr}`);
    });
  });
}
