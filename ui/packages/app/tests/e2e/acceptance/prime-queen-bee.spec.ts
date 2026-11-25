/**
 * prime-queen-bee.spec.ts — the fleet-to-fleet journey.
 *
 * One fleet directs another in its workspace and reports what came back. The
 * two fleets, what a person types and the lines this journey proves are
 * `tests/fixtures/fleets/queen-bee.json` and `busy-bee.json`; each test is
 * named by the line it asserts.
 *
 * busy-bee is created first and is told today's word, which is this run's
 * token. Only busy-bee holds it, so the word in queen-bee's chat is something
 * queen-bee could only have read from busy-bee's answer.
 */
import { expect, test } from "@playwright/test";
import { z } from "zod";
import type { TurnDetail } from "@/lib/api/turns";
import { fixtureSubject, signInAs } from "./fixtures/auth";
import { POST_DEPLOY_TAG } from "./fixtures/constants";
import { assertPassed, classifyTerminalTurn, JOURNEY_LEG } from "./fixtures/execution";
import { workspaceHref } from "./fixtures/nav";
import { observed, pollFor, uniqueTag } from "./fixtures/observation";
import { createPrimeFleet, entry, PRIME_FLEET, primeFleet, proofs, type PrimedFleet } from "./fixtures/prime-fleets";
import { callTo, PRIME_PERSON, say, settledTurn, threadOf } from "./fixtures/prime-walk";
import { getDefaultWorkspaceId } from "./fixtures/seed";
import { purgeFixtureFleet } from "./fixtures/teardown";

const queen = primeFleet(PRIME_FLEET.queenBee);
const busy = primeFleet(PRIME_FLEET.busyBee);
const queenProof = proofs(queen, 3);
const busyProof = proofs(busy, 2);
const STEP_TIMEOUT_MS = 480_000;
const RENDER_TIMEOUT_MS = 15_000;
// busy-bee's run, and queen-bee reading it back: two runs behind one queue.
const ANSWER_TIMEOUT_MS = 240_000;
const FLEET_STEER = "fleet_steer";
const FLEET_ACTOR_PREFIX = "fleet:";
const PERSON_ACTOR_PREFIX = "steer:";
const CHAT_LABEL = "Fleet chat";
/** What a fleet's message stores beside its words (`af_wire::turn::FleetSteerBody`). */
const SteerBody = z.object({sender_fleet_id: z.string(), sender_fleet_name: z.string(), sender_turn_id: z.string()});

test.describe.serial(`prime fleets ${queen.name} and ${busy.name}`, {tag: POST_DEPLOY_TAG}, () => {
  test.setTimeout(STEP_TIMEOUT_MS);
  let workspaceId = "";
  const word = uniqueTag();
  let directed: PrimedFleet | null = null;
  let director: PrimedFleet | null = null;
  // The turn queen-bee ran for the person, and the turn it put on busy-bee.
  let walk: {asked: TurnDetail; steered: TurnDetail} | null = null;

  const fleets = (): {director: PrimedFleet; directed: PrimedFleet} => {
    if (director === null || directed === null) throw new Error("the two fleets were not created; nothing to walk");
    return {director, directed};
  };
  const walked = (): {asked: TurnDetail; steered: TurnDetail} => {
    if (walk === null) throw new Error(`${queen.name} steered nothing; nothing to read`);
    return walk;
  };

  test.beforeAll(async () => {
    test.setTimeout(STEP_TIMEOUT_MS);
    workspaceId = await getDefaultWorkspaceId(PRIME_PERSON);
    directed = await observed(JOURNEY_LEG.install, () =>
      createPrimeFleet(PRIME_PERSON, workspaceId, busy, `${busy.first_message} Today's word is ${word}.`));
    director = await observed(JOURNEY_LEG.install, () => createPrimeFleet(PRIME_PERSON, workspaceId, queen));
  });

  test.afterAll(async () => {
    for (const fleet of [director, directed]) if (fleet) await purgeFixtureFleet(PRIME_PERSON, workspaceId, fleet.id);
  });

  test(queenProof(0), async ({page}) => {
    await signInAs(page, PRIME_PERSON);
    expect(entry(queen, queen.steers, 0, "steered fleet")).toBe(busy.name);
    const asked = await say(page, workspaceId, fleets().director.id, `${entry(queen, queen.follow_ups, 0, "follow-up")} ` +
      `The fleet named ${fleets().directed.name} has the fleet_id ${fleets().directed.id}. After ${FLEET_STEER}, read it with fleet_activity ` +
      "until its newest event carries its answer, reading again while it has not answered, and include the word it gave in your reply.");
    assertPassed(classifyTerminalTurn(asked));
    expect(callTo(asked, FLEET_STEER)).toMatchObject({fleet_id: fleets().directed.id});
    // The steer became a turn of busy-bee's own, after the one that created it.
    const steered = await settledTurn(workspaceId, fleets().directed.id,
      (turn) => turn.turn_id !== fleets().directed.turnId && turn.actor.startsWith(FLEET_ACTOR_PREFIX), ANSWER_TIMEOUT_MS);
    if (steered === null) throw new Error(`${queen.name} called ${FLEET_STEER} and ${busy.name} holds no turn from a fleet`);
    expect(SteerBody.parse(JSON.parse(steered.request_json)).sender_fleet_id).toBe(fleets().director.id);
    walk = {asked, steered};
  });

  test(queenProof(1), () => {
    const {asked, steered} = walked();
    expect(steered.actor).toBe(`${FLEET_ACTOR_PREFIX}${fleets().director.id}`);
    expect(steered.actor).not.toBe(`${PERSON_ACTOR_PREFIX}${fixtureSubject(PRIME_PERSON)}`);
    expect(SteerBody.parse(JSON.parse(steered.request_json))).toEqual({
      sender_fleet_id: fleets().director.id, sender_fleet_name: fleets().director.name, sender_turn_id: asked.turn_id,
    });
  });

  test(busyProof(0), () => {
    const {steered} = walked();
    assertPassed(classifyTerminalTurn(steered));
    expect(steered.response_text).toContain(word);
  });

  test(busyProof(1), async ({page}) => {
    await signInAs(page, PRIME_PERSON);
    await page.goto(workspaceHref(workspaceId, `fleets/${fleets().directed.id}`));
    // The message row is labelled with the fleet that sent it; no other row in this chat names it.
    await expect(page.getByLabel(CHAT_LABEL).getByText(fleets().director.name).first()).toBeVisible({timeout: RENDER_TIMEOUT_MS});
  });

  test(queenProof(2), async ({page}) => {
    await signInAs(page, PRIME_PERSON);
    const {asked} = walked();
    // The word reaches queen-bee's chat in the turn the person asked, or in the one busy-bee's answer woke.
    const reported = await pollFor(async () => (await threadOf(workspaceId, fleets().director.id))
      .find((turn) => turn.turn_id !== fleets().director.turnId && (turn.response_text ?? "").includes(word)) ?? null, ANSWER_TIMEOUT_MS);
    if (reported === null) throw new Error(`${queen.name} never reported the word ${busy.name} was given; its reply to the person: ${asked.response_text ?? "none"}`);
    assertPassed(classifyTerminalTurn(reported));
    await page.goto(workspaceHref(workspaceId, `fleets/${fleets().director.id}`));
    await expect(page.getByLabel(CHAT_LABEL).getByText(word).first()).toBeVisible({timeout: RENDER_TIMEOUT_MS});
  });
});
