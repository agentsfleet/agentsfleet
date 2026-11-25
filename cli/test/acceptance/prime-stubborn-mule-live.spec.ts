// The task journey, from a terminal: stubborn-mule starts a task from a
// person's message, and the task's own turns follow in its chat.
//
// The fleet's name and messages are `tests/fixtures/fleets/stubborn-mule.json`.
// No command lists a fleet's tasks, so the journey reads the task where the
// terminal does show it: `fleet turns`, where each run of a task is a turn sent
// by `durable_task:<task id>`.

import { describe, it, beforeAll } from "bun:test";
import assert from "node:assert/strict";

import { ACCEPTANCE_TARGET_ENV } from "./fixtures/constants.ts";
import { entry, primeFleet as primeFixture } from "./fixtures/prime-fleets.ts";
import { isLive, ownWorkspace, primeFleet, primeSession } from "./fixtures/prime-session.ts";
import { awaitTurn, processed, steer, turnsOf, type Turn } from "./fixtures/prime-turns.ts";

const fixture = primeFixture("stubborn-mule");
const startTask = entry(fixture.follow_ups, 0, "follow-up");
const WORKSPACE = `fixture-cli-${fixture.journey}`;

const SETUP_TIMEOUT_MS = 360_000;
// The message's own run, then the task's first run: prepared, run, and judged by the decision model.
const TASK_TIMEOUT_MS = 420_000;
const JOURNEY_TIMEOUT_MS = 720_000;
// `afd_fleet::durable_task::continuation`: the sender and the kind of a task's own turn.
const TASK_ACTOR_PREFIX = "durable_task:";
const TURN_KIND_CONTINUATION = "continuation";

const taskTurn = (turn: Turn): boolean => turn.actor.startsWith(TASK_ACTOR_PREFIX);

if (!isLive) {
  describe("prime-stubborn-mule-live.spec.ts", () => {
    it.skip(`requires ${ACCEPTANCE_TARGET_ENV} to be an https URL`, () => {});
  });
} else {
  describe("stubborn-mule: a fleet starts a task that runs in its own turns", () => {
    const session = primeSession(ownWorkspace(WORKSPACE));
    let fleetId = "";

    beforeAll(async () => {
      const created = await primeFleet(session, fixture);
      fleetId = created.fleet_id;
      await awaitTurn(session, fleetId, processed((turn) => turn.turn_id === created.turn_id), "answered first turn");
    }, SETUP_TIMEOUT_MS);

    it("a first message that asks for no task starts none", async () => {
      assert.ok(fleetId, "the fleet was not created");
      assert.deepEqual((await turnsOf(session, fleetId)).filter(taskTurn), []);
    });

    it("the follow-up starts one task, whose first run is a turn the task sent", async () => {
      assert.ok(fleetId, "the fleet was not created");
      const askedIn = await steer(session, fleetId, startTask);

      // The fleet's run is offered the task tools only on a runner that can run
      // tasks, with a decision model to judge them; without either this never comes.
      const ran = await awaitTurn(session, fleetId, processed(taskTurn), "answered task turn", TASK_TIMEOUT_MS);
      assert.equal(ran.event_type, TURN_KIND_CONTINUATION);
      assert.notEqual(ran.turn_id, askedIn, "the task runs in its own turn, not in the message that started it");

      const tasks = new Set((await turnsOf(session, fleetId)).filter(taskTurn).map((turn) => turn.actor));
      assert.equal(tasks.size, 1, `one message starts one task: ${JSON.stringify([...tasks])}`);
    }, JOURNEY_TIMEOUT_MS);
  });
}
