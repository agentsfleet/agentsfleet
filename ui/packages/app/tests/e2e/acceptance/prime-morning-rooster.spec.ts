/**
 * prime-morning-rooster.spec.ts — the first-message journey.
 *
 * One message makes a fleet that answers in its own chat. The fleet's name,
 * its first message, the message a person sends next and the four lines this
 * journey proves are `tests/fixtures/fleets/morning-rooster.json`; each test
 * is named by the line it asserts. Every message carries this run's token, and
 * the reply is asserted through the turn the API returns, then on the page.
 */
import { readFileSync } from "node:fs";
import { rm } from "node:fs/promises";
import { expect, test } from "@playwright/test";
import { z } from "zod";
import { FleetDetailSchema } from "@/lib/api/fleet-detail";
import { fixtureSubject, signInAs } from "./fixtures/auth";
import { clientFor } from "./fixtures/api-client";
import { awaitTerminalTurn } from "./fixtures/chat-walk";
import { cliEnv, deleteCliKey, makeCliStateDir, mintCliKey, spawnAgentsfleet, writeCliState } from "./fixtures/cli-runner";
import { JWT_CACHE_PATH, POST_DEPLOY_TAG } from "./fixtures/constants";
import { assertPassed, classifyTerminalTurn, JOURNEY_LEG, TURN_KIND_CHAT } from "./fixtures/execution";
import { createFixtureFleet, runtimeModelRef } from "./fixtures/fleet-create";
import { expectSidebarFleetState } from "./fixtures/lifecycle";
import { workspaceHref } from "./fixtures/nav";
import { createFleetViaUI } from "./fixtures/new-fleet";
import { observed, uniqueTag } from "./fixtures/observation";
import { entry, PRIME_FLEET, primeFleet, proofs } from "./fixtures/prime-fleets";
import { messageOf, PRIME_PERSON, say, threadOf } from "./fixtures/prime-walk";
import { probeReply, tokenMessage } from "./fixtures/probe";
import { getDefaultWorkspaceId } from "./fixtures/seed";
import { documentIsMarked, markDocument } from "./fixtures/soft-navigation";
import { purgeFixtureFleet } from "./fixtures/teardown";

const fixture = primeFleet(PRIME_FLEET.morningRooster);
const proof = proofs(fixture, 4);
// One provider round trip on a runner, with the queue in front of it.
const ANSWER_TIMEOUT_MS = 180_000;
const RENDER_TIMEOUT_MS = 15_000;
const STEP_TIMEOUT_MS = 420_000;
const CHAT_LABEL = "Fleet chat";
const STATUS_ACTIVE = "active";
const STATE_DIR_PREFIX = "prime-rooster-cli-";
const cacheSchema = z.record(z.string(), z.object({sessionJwt: z.string()}));
// `fleet create --json` prints what the create answered: the fleet and its first turn.
const createdSchema = z.object({fleet_id: z.uuidv7(), turn_id: z.uuidv7()});

/** `agentsfleet fleet create` with a key minted for this run and removed after it. */
async function createFromCommandLine(workspaceId: string, name: string, message: string) {
  const api = process.env.NEXT_PUBLIC_API_URL;
  if (!api) throw new Error("NEXT_PUBLIC_API_URL is required");
  const jwt = cacheSchema.parse(JSON.parse(readFileSync(JWT_CACHE_PATH, "utf8")))[PRIME_PERSON]?.sessionJwt;
  if (!jwt) throw new Error("the signed-in person's session is missing from the fixture cache");
  const key = await mintCliKey(api, jwt, `${STATE_DIR_PREFIX}${uniqueTag()}`);
  const {root, stateDir} = await makeCliStateDir(STATE_DIR_PREFIX);
  try {
    await writeCliState(stateDir, workspaceId, key.key, api, "prime-workspace");
    const model = await runtimeModelRef({sessionJwt: key.key}, workspaceId);
    const env = cliEnv({AGENTSFLEET_STATE_DIR: stateDir, AGENTSFLEET_API_URL: api, AGENTSFLEET_API_KEY: key.key});
    const result = await spawnAgentsfleet(["--json", "fleet", "create", message, "--name", name, "--model", model], env, ANSWER_TIMEOUT_MS);
    return {result, model};
  } finally {
    await deleteCliKey(api, jwt, key.id);
    await rm(root, {recursive: true, force: true});
  }
}

test.describe.serial(`prime fleet ${fixture.name}`, {tag: POST_DEPLOY_TAG}, () => {
  test.setTimeout(STEP_TIMEOUT_MS);
  let workspaceId = "";
  const made: string[] = [];
  // The fleet the API made under the fixture's name, and the one the browser made.
  let named: {id: string; turnId: string} | null = null;
  let typed: {id: string; message: string; token: string} | null = null;

  test.afterAll(async () => {
    for (const fleetId of made) await purgeFixtureFleet(PRIME_PERSON, workspaceId, fleetId);
  });

  test(proof(0), async () => {
    workspaceId = await getDefaultWorkspaceId(PRIME_PERSON);
    const token = uniqueTag();
    const message = tokenMessage(fixture.first_message, token);
    const created = await observed(JOURNEY_LEG.install, () => createFixtureFleet(PRIME_PERSON, workspaceId, {name: fixture.name, message}));
    made.push(created.fleet.id);
    named = {id: created.fleet.id, turnId: created.turnId};
    expect(created.fleet.status).toBe(STATUS_ACTIVE);
    expect(created.fleet.name === fixture.name || created.fleet.name.startsWith(`${fixture.name}-`), `named ${created.fleet.name}`).toBe(true);

    // Its first turn is the one turn on the thread: the person's, carrying the message.
    const thread = await threadOf(workspaceId, created.fleet.id);
    expect(thread.map((turn) => turn.turn_id)).toEqual([created.turnId]);
    const first = entry(fixture, thread, 0, "first turn");
    expect(first.actor).toBe(`steer:${fixtureSubject(PRIME_PERSON)}`);
    expect(first.event_type).toBe(TURN_KIND_CHAT);
    expect(messageOf(first)).toBe(message);

    const answered = await awaitTerminalTurn(workspaceId, created.fleet.id);
    assertPassed(classifyTerminalTurn(answered));
    expect(answered.turn_id).toBe(created.turnId);
    expect(answered.response_text).toContain(probeReply(token));
  });

  test(proof(1), async ({page}) => {
    await signInAs(page, PRIME_PERSON);
    const token = uniqueTag();
    const message = tokenMessage(fixture.first_message, token);
    const fleetId = await createFleetViaUI(page, workspaceId, message);
    made.push(fleetId);
    typed = {id: fleetId, message, token};
    // The page that took the message is the fleet's chat, and the reply arrives in it with no reload.
    await markDocument(page);
    const chat = page.getByLabel(CHAT_LABEL);
    await expect(chat.getByText(probeReply(token)).first()).toBeVisible({timeout: ANSWER_TIMEOUT_MS});
    expect(await documentIsMarked(page)).toBe(true);
    await expect(page).toHaveURL(workspaceHref(workspaceId, `fleets/${fleetId}`));
    await expectSidebarFleetState(page, fleetId, STATUS_ACTIVE);
  });

  test(proof(2), async ({page}) => {
    if (typed === null) throw new Error("the browser made no fleet; nothing to compare");
    // The browser's fleet holds what the API's does: active, one person's turn, answered.
    const browser = FleetDetailSchema.parse(await clientFor(PRIME_PERSON).get<unknown>(`/v1/workspaces/${workspaceId}/fleets/${typed.id}`));
    expect(browser.status).toBe(STATUS_ACTIVE);
    const [turn, ...others] = await threadOf(workspaceId, typed.id);
    expect(others).toEqual([]);
    expect(turn && messageOf(turn)).toBe(typed.message);
    expect(turn?.response_text).toContain(probeReply(typed.token));

    // The command line makes the same thing from the same words; the name is the fixture's with a tag, because the fixture's is taken.
    const token = uniqueTag();
    const name = `${fixture.name}-${token}`;
    const message = tokenMessage(fixture.first_message, token);
    const {result, model} = await createFromCommandLine(workspaceId, name, message);
    expect(result.code, result.stdout + result.stderr).toBe(0);
    const answer = createdSchema.parse(JSON.parse(result.stdout));
    made.push(answer.fleet_id);
    const fleet = FleetDetailSchema.parse(await clientFor(PRIME_PERSON).get<unknown>(`/v1/workspaces/${workspaceId}/fleets/${answer.fleet_id}`));
    expect(fleet).toMatchObject({name, status: STATUS_ACTIVE, model_ref: model});
    const answered = await awaitTerminalTurn(workspaceId, answer.fleet_id);
    assertPassed(classifyTerminalTurn(answered));
    expect(answered.turn_id).toBe(answer.turn_id);
    expect(answered.response_text).toContain(probeReply(token));

    await signInAs(page, PRIME_PERSON);
    await page.goto(workspaceHref(workspaceId, `fleets/${answer.fleet_id}`));
    await expect(page.getByRole("heading", {name, exact: true})).toBeVisible();
    await expect(page.getByLabel(CHAT_LABEL).getByText(probeReply(token)).first()).toBeVisible({timeout: RENDER_TIMEOUT_MS});
  });

  test(proof(3), async ({page}) => {
    if (named === null) throw new Error("the first message made no fleet; nothing to message");
    await signInAs(page, PRIME_PERSON);
    const token = uniqueTag();
    const later = await say(page, workspaceId, named.id, tokenMessage(entry(fixture, fixture.follow_ups, 0, "follow-up"), token));
    assertPassed(classifyTerminalTurn(later));
    expect(later.response_text).toContain(probeReply(token));
    // Same chat: the thread holds the first turn and this one, and nothing else.
    const thread = await threadOf(workspaceId, named.id);
    expect(thread.map((turn) => turn.turn_id)).toEqual([later.turn_id, named.turnId]);
    await expect(page.getByLabel(CHAT_LABEL).getByText(probeReply(token)).first()).toBeVisible({timeout: RENDER_TIMEOUT_MS});
  });
});
