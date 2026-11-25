/**
 * prime-chatty-parrot.spec.ts — the Slack journey. Runs against a deployed
 * stack only: Slack delivers its events there and never to a stack a machine
 * started for itself.
 *
 * A fleet answers a mention in the thread it came from. The fleet, what a
 * person types and the four lines this journey proves are
 * `tests/fixtures/fleets/chatty-parrot.json`; each test is named by the line
 * it asserts.
 *
 * The mention is a real message, posted in the channel as the test person;
 * Slack signs and delivers the event, and the reply is read back from the real
 * thread. The daemon drops a post by a bot, so the person's own token posts it.
 * The journey reads the channel from `ACCEPTANCE_SLACK_CHANNEL_ID`, the
 * person's token from `ACCEPTANCE_SLACK_USER_TOKEN`, and the member id the
 * connected app answers as from `ACCEPTANCE_SLACK_BOT_USER_ID`.
 */
import * as crypto from "node:crypto";
import { expect, test } from "@playwright/test";
import { z } from "zod";
import type { ConnectorCatalogEntry } from "@/lib/api/connectors";
import type { TurnDetail } from "@/lib/api/turns";
import { TRIGGER_KIND, TriggerSchema } from "@/lib/api/fleet-detail";
import { clientFor } from "./fixtures/api-client";
import { POST_DEPLOY_TAG } from "./fixtures/constants";
import { assertPassed, classifyTerminalTurn, JOURNEY_LEG } from "./fixtures/execution";
import { CONNECTOR_SERVICE_SLACK } from "./fixtures/grants";
import { callSlack, requiredVariable, requiredVariables } from "./fixtures/live-services";
import { observed, pollFor, uniqueTag } from "./fixtures/observation";
import { createPrimeFleet, entry, PRIME_FLEET, primeFleet, proofs, wake, type PrimedFleet } from "./fixtures/prime-fleets";
import { PRIME_PERSON, settledTurn, threadOf, toolsCalled } from "./fixtures/prime-walk";
import { getDefaultWorkspaceId } from "./fixtures/seed";
import { purgeFixtureFleet } from "./fixtures/teardown";

const fixture = primeFleet(PRIME_FLEET.chattyParrot);
const proof = proofs(fixture, 4);
const subscription = wake(fixture, "mention");
const CHANNEL_VARIABLE = "ACCEPTANCE_SLACK_CHANNEL_ID";
const TOKEN_VARIABLE = "ACCEPTANCE_SLACK_USER_TOKEN";
const BOT_VARIABLE = "ACCEPTANCE_SLACK_BOT_USER_ID";
const STEP_TIMEOUT_MS = 480_000;
// Slack's delivery, then one run on a runner, then the reply's own delivery.
const WAKE_TIMEOUT_MS = 240_000;
const REPLY_TIMEOUT_MS = 120_000;
const REQUEST_TIMEOUT_MS = 30_000;
const TRIGGER_ADD = "trigger_add";
const SLACK_ACTOR_PREFIX = "slack:";
const HTTP_UNAUTHORIZED = 401;
/** `UZ-WH-010`: a delivery whose signature does not verify (`af_core::error_code`). */
const SIGNATURE_INVALID = "UZ-WH-010";
const TriggersSchema = z.object({items: z.array(TriggerSchema)});
// Slack stamps `bot_id` on a message it stores as an app's.
const PostedSchema = z.object({ts: z.string().min(1), message: z.object({bot_id: z.string().optional()})});
const RepliesSchema = z.object({messages: z.array(z.object({ts: z.string(), thread_ts: z.string().optional(), user: z.string().optional(), text: z.string().optional()}))});
const RefusedSchema = z.object({error_code: z.string()});

test.describe.serial(`prime fleet ${fixture.name}`, {tag: POST_DEPLOY_TAG}, () => {
  test.setTimeout(STEP_TIMEOUT_MS);
  let workspaceId = "";
  let channel = "";
  let token = "";
  let bot = "";
  let fleet: PrimedFleet | null = null;
  const word = uniqueTag();
  let postedTs = "";
  let said: {ts: string; woken: TurnDetail} | null = null;

  const primed = (): PrimedFleet => {
    if (fleet === null) throw new Error(`${fixture.name} was not created; nothing to walk`);
    return fleet;
  };
  const mention = () => {
    if (said === null) throw new Error("no mention woke the fleet; nothing to read");
    return said;
  };
  const fromSlack = async (): Promise<TurnDetail[]> =>
    (await threadOf(workspaceId, primed().id)).filter((turn) => turn.actor.startsWith(SLACK_ACTOR_PREFIX));

  test.beforeAll(async () => {
    test.setTimeout(STEP_TIMEOUT_MS);
    const given = requiredVariables([CHANNEL_VARIABLE, TOKEN_VARIABLE, BOT_VARIABLE]);
    channel = given[CHANNEL_VARIABLE];
    token = given[TOKEN_VARIABLE];
    bot = given[BOT_VARIABLE];
    workspaceId = await getDefaultWorkspaceId(PRIME_PERSON);
    const catalogue = await clientFor(PRIME_PERSON).get<ConnectorCatalogEntry[]>(`/v1/workspaces/${workspaceId}/connectors`);
    // Connecting is a person's consent in Slack, done once for the fixture workspace; no journey can give it.
    if (!catalogue.some((row) => row.id === CONNECTOR_SERVICE_SLACK && row.connected)) {
      throw new Error("the fixture workspace has not connected Slack; connect the deployment's Slack app to it in Connected apps");
    }
    fleet = await observed(JOURNEY_LEG.install, () =>
      createPrimeFleet(PRIME_PERSON, workspaceId, fixture, `${fixture.first_message} The channel is ${channel}.`));
  });

  test.afterAll(async () => {
    if (fleet) await purgeFixtureFleet(PRIME_PERSON, workspaceId, fleet.id);
    // The person's own message is removed; the fleet's reply under it is the bot's to remove and stays.
    if (postedTs) await callSlack(token, "chat.delete", {channel, ts: postedTs}, z.object({})).catch(() => undefined);
  });

  test(proof(0), async () => {
    const first = entry(fixture, (await threadOf(workspaceId, primed().id)).filter((turn) => turn.turn_id === primed().turnId), 0, "first turn");
    assertPassed(classifyTerminalTurn(first));
    expect(toolsCalled(first)).toContain(TRIGGER_ADD);
    const triggers = TriggersSchema.parse(await clientFor(PRIME_PERSON).get<unknown>(`/v1/workspaces/${workspaceId}/fleets/${primed().id}/triggers`)).items;
    expect(triggers.map((row) => [row.kind, row.source, row.channel])).toEqual([[TRIGGER_KIND.mention, subscription.source, channel]]);
  });

  test(proof(1), async () => {
    expect(await fromSlack(), "no mention has reached the fleet yet").toEqual([]);
    // The fleet's name leads the words, so the mention is this fleet's whichever others listen on the channel.
    const posted = await callSlack(token, "chat.postMessage", {channel, text: `<@${bot}> ${primed().name} the word is ${word}`}, PostedSchema);
    postedTs = posted.ts;
    if (posted.message.bot_id !== undefined) {
      throw new Error(`Slack stored the mention ${TOKEN_VARIABLE} posted as a bot's message, and the daemon drops a bot's post; the token must post as a person`);
    }
    const woken = await settledTurn(workspaceId, primed().id, (turn) => turn.actor.startsWith(SLACK_ACTOR_PREFIX), WAKE_TIMEOUT_MS);
    if (woken === null) throw new Error(`the mention posted at ${posted.ts} woke no turn in ${WAKE_TIMEOUT_MS / 1000} s`);
    said = {ts: posted.ts, woken};
    assertPassed(classifyTerminalTurn(woken));
    expect(woken.response_text).toContain(word);
    expect((await fromSlack()).map((turn) => turn.turn_id), "one mention is one turn").toEqual([woken.turn_id]);
  });

  test(proof(2), async () => {
    const {ts} = mention();
    const reply = await pollFor(async () => {
      const {messages} = await callSlack(token, "conversations.replies", {channel, ts}, RepliesSchema);
      return messages.find((message) => message.ts !== ts && message.user === bot) ?? null;
    }, REPLY_TIMEOUT_MS);
    if (reply === null) throw new Error(`the thread under ${ts} holds no reply from the bot after ${REPLY_TIMEOUT_MS / 1000} s`);
    expect(reply.thread_ts).toBe(ts);
    expect(reply.text).toContain(word);
  });

  test(proof(3), async () => {
    const api = requiredVariable("NEXT_PUBLIC_API_URL");
    // A mention nobody signed: the daemon refuses it before it reads a word of it.
    const forged = JSON.stringify({type: "event_callback", event: {type: "app_mention", channel, text: `<@${bot}> ${primed().name}`}});
    const response = await fetch(`${api}/v1/connectors/${CONNECTOR_SERVICE_SLACK}/events`, {
      method: "POST", body: forged, signal: AbortSignal.timeout(REQUEST_TIMEOUT_MS),
      headers: {"content-type": "application/json", "x-slack-request-timestamp": String(Math.floor(Date.now() / 1000)),
        "x-slack-signature": `v0=${crypto.randomBytes(32).toString("hex")}`},
    });
    expect(response.status).toBe(HTTP_UNAUTHORIZED);
    expect(RefusedSchema.parse(await response.json()).error_code).toBe(SIGNATURE_INVALID);
    expect((await fromSlack()).map((turn) => turn.turn_id)).toEqual([mention().woken.turn_id]);
    // The other half needs a real mention in a second channel the bot is in, and no variable names one.
    test.fixme(true, "a mention in another channel is unproved: the deployment names one Slack channel");
  });
});
