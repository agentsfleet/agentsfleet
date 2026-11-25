/**
 * prime-night-owl.spec.ts — the wake-ups journey.
 *
 * A fleet that wakes on its own: it schedules itself, and it listens for a
 * signed webhook. The fleet, what a person types, what it wakes on and the
 * four lines this journey proves are `tests/fixtures/fleets/night-owl.json`;
 * each test is named by the line it asserts.
 *
 * The scheduled wake is the deployment's scheduler calling the daemon; this
 * journey only waits for it. The webhook's sender is whoever holds the signing
 * secret, and here that is this journey: it stores the secret, signs its own
 * small body with it, and posts to the address the fleet's trigger names.
 */
import * as crypto from "node:crypto";
import { expect, test } from "@playwright/test";
import { z } from "zod";
import type { TurnDetail } from "@/lib/api/turns";
import { TRIGGER_KIND, TriggerSchema } from "@/lib/api/fleet-detail";
import { SCHEDULE_SYNC, ScheduleSchema, type Schedule } from "@/lib/api/schedule-types";
import { signInAs } from "./fixtures/auth";
import { clientFor } from "./fixtures/api-client";
import { POST_DEPLOY_TAG } from "./fixtures/constants";
import { assertPassed, classifyTerminalTurn, failWith, JOURNEY_LEG, turnHasSettled, VERDICT_KIND } from "./fixtures/execution";
import { workspaceHref } from "./fixtures/nav";
import { observed, POLL_INTERVAL_MS, pollFor, uniqueTag } from "./fixtures/observation";
import { createPrimeFleet, entry, PRIME_FLEET, primeFleet, proofs, wake, type PrimedFleet } from "./fixtures/prime-fleets";
import { PRIME_PERSON, say, settledTurn, threadOf, toolsCalled } from "./fixtures/prime-walk";
import { probeMessage, probeReply, tokenMessage } from "./fixtures/probe";
import { getDefaultWorkspaceId } from "./fixtures/seed";
import { purgeFixtureFleet } from "./fixtures/teardown";

const fixture = primeFleet(PRIME_FLEET.nightOwl);
const proof = proofs(fixture, 4);
const schedule = wake(fixture, "schedule");
const webhook = wake(fixture, "webhook");
const STEP_TIMEOUT_MS = 600_000;
const RENDER_TIMEOUT_MS = 15_000;
// The scheduler takes the schedule within one read of the daemon having stored it.
const SYNC_TIMEOUT_MS = 60_000;
// Two scheduler minutes: the first may already be under way when the schedule lands.
const WAKE_TIMEOUT_MS = 180_000;
// One run on a runner with one provider round trip, behind the queue.
const RUN_TIMEOUT_MS = 180_000;
// Long enough for a second delivery to have become a second turn, in polls.
const REPLAY_GRACE_MS = POLL_INTERVAL_MS * 5;
const CRON_ACTOR_PREFIX = "cron:";
const TURN_KIND_CRON = "cron";
const TURN_KIND_WEBHOOK = "webhook";
const CRON_ADD = "cron_add";
const TRIGGER_ADD = "trigger_add";
const CHAT_LABEL = "Fleet chat";
const WAKE_TITLE = "Woke up on schedule";
// How the webhook's sender signs: the body's HMAC-SHA256 in hex, prefixed, on a header of its own.
const VERIFIER = {scheme: "body-hex-sha256", header: "x-owl-signature"} as const;
const SIGNATURE_PREFIX = "sha256=";
const WEBHOOK_SECRET_FIELD = "webhook_secret";
const HTTP_ACCEPTED = 202;
const HTTP_UNAUTHORIZED = 401;
/** `UZ-WH-010`: a delivery whose signature does not verify (`af_core::error_code`). */
const SIGNATURE_INVALID = "UZ-WH-010";
const SchedulesSchema = z.object({schedules: z.array(ScheduleSchema)});
const TriggersSchema = z.object({items: z.array(TriggerSchema)});
const AcceptedSchema = z.object({turn_id: z.string().min(1), replayed: z.boolean()});
const RefusedSchema = z.object({error_code: z.string()});

function signature(secret: string, body: string): string {
  return `${SIGNATURE_PREFIX}${crypto.createHmac("sha256", secret).update(body).digest("hex")}`;
}

/** One delivery to the fleet's webhook address. The route takes no bearer: its proof is the signature. */
async function deliver(url: string, body: string, signed: string): Promise<{status: number; answer: unknown}> {
  const response = await fetch(url, {
    method: "POST", body, signal: AbortSignal.timeout(RENDER_TIMEOUT_MS),
    headers: {"content-type": "application/json", [VERIFIER.header]: signed},
  });
  return {status: response.status, answer: await response.json()};
}

test.describe.serial(`prime fleet ${fixture.name}`, {tag: POST_DEPLOY_TAG}, () => {
  test.setTimeout(STEP_TIMEOUT_MS);
  let workspaceId = "";
  let fleet: PrimedFleet | null = null;
  const secret = crypto.randomBytes(24).toString("hex");
  let delivered: {url: string; body: string; turnId: string} | null = null;

  const primed = (): PrimedFleet => {
    if (fleet === null) throw new Error(`${fixture.name} was not created; nothing to walk`);
    return fleet;
  };
  const fleetPath = (): string => `/v1/workspaces/${workspaceId}/fleets/${primed().id}`;
  const schedules = async (): Promise<Schedule[]> =>
    SchedulesSchema.parse(await observed(JOURNEY_LEG.observe, () => clientFor(PRIME_PERSON).get<unknown>(`${fleetPath()}/schedules`))).schedules;
  const webhookTurns = async (): Promise<TurnDetail[]> =>
    (await threadOf(workspaceId, primed().id)).filter((turn) => turn.event_type === TURN_KIND_WEBHOOK);

  test.beforeAll(async () => {
    test.setTimeout(STEP_TIMEOUT_MS);
    workspaceId = await getDefaultWorkspaceId(PRIME_PERSON);
    await clientFor(PRIME_PERSON).post(`/v1/workspaces/${workspaceId}/secrets`,
      {name: webhook.secret_name, data: {[WEBHOOK_SECRET_FIELD]: secret}});
    fleet = await observed(JOURNEY_LEG.install, () => createPrimeFleet(PRIME_PERSON, workspaceId, fixture));
  });

  test.afterAll(async () => {
    // Purging the fleet removes its schedule, so nothing keeps waking a runner.
    if (fleet) await purgeFixtureFleet(PRIME_PERSON, workspaceId, fleet.id);
    await clientFor(PRIME_PERSON).delete(`/v1/workspaces/${workspaceId}/secrets/${encodeURIComponent(webhook.secret_name)}`).catch(() => undefined);
  });

  test(proof(0), async ({page}) => {
    await signInAs(page, PRIME_PERSON);
    const token = uniqueTag();
    // What each wake is asked: the fixture's standing guidance, with this run's token.
    const wakeMessage = tokenMessage(entry(fixture, fixture.remembers, 0, "memory"), token);
    const scheduleArgs = {cron: schedule.cron, timezone: "UTC", message: wakeMessage};
    const asked = await say(page, workspaceId, primed().id, `${entry(fixture, fixture.follow_ups, 0, "follow-up")} ` +
      `Call ${CRON_ADD} once with these exact arguments. Copy the entire message into the tool call.\n${JSON.stringify(scheduleArgs)}`);
    assertPassed(classifyTerminalTurn(asked));
    expect(toolsCalled(asked)).toContain(CRON_ADD);
    const synced = await pollFor(async () => {
      const rows = await schedules();
      expect(rows.map((row) => row.cron), "the fleet added the one schedule it was asked for").toEqual([schedule.cron]);
      return rows.find((row) => row.sync !== SCHEDULE_SYNC.syncing) ?? null;
    }, SYNC_TIMEOUT_MS);
    expect(synced, "the scheduler never answered for the schedule").toMatchObject({sync: SCHEDULE_SYNC.synced});
    expect(synced?.message).toContain(token);

    const actor = `${CRON_ACTOR_PREFIX}${synced?.schedule_id}`;
    const woken = await settledTurn(workspaceId, primed().id, (turn) => turn.actor === actor, WAKE_TIMEOUT_MS + RUN_TIMEOUT_MS);
    // The schedule is stored and the scheduler holds it; a wake that never arrives is the scheduler not reaching the daemon.
    if (woken === null) failWith({kind: VERDICT_KIND.environment, leg: JOURNEY_LEG.run, detail: `the scheduler holds schedule ${synced?.schedule_id} and no wake reached the fleet`});
    expect(woken.event_type).toBe(TURN_KIND_CRON);
    assertPassed(classifyTerminalTurn(woken));
    expect(woken.response_text).toContain(probeReply(token));
    await page.goto(workspaceHref(workspaceId, `fleets/${primed().id}`));
    const chat = page.getByLabel(CHAT_LABEL);
    await expect(chat.getByText(WAKE_TITLE).first()).toBeVisible({timeout: RENDER_TIMEOUT_MS});
    await expect(chat.getByText(probeReply(token)).first()).toBeVisible({timeout: RENDER_TIMEOUT_MS});
  });

  test(proof(1), async ({page}) => {
    await signInAs(page, PRIME_PERSON);
    const woken = (await threadOf(workspaceId, primed().id)).filter((turn) => turn.event_type === TURN_KIND_CRON);
    expect(woken.length, "the chat already holds a scheduled wake").toBeGreaterThan(0);
    // A person removes what the fleet added. With no wake a minute, the message below starts a run of its own
    // instead of joining a wake's, and the remaining steps count only the turns they cause.
    for (const row of await schedules()) await clientFor(PRIME_PERSON).delete(`${fleetPath()}/schedules/${row.schedule_id}`);
    await expect.poll(async () => (await schedules()).length, {timeout: SYNC_TIMEOUT_MS}).toBe(0);
    await expect.poll(async () => (await threadOf(workspaceId, primed().id)).every((turn) => turnHasSettled(turn.status)),
      {message: "a wake was still running", timeout: RUN_TIMEOUT_MS}).toBe(true);
    const token = uniqueTag();
    const answered = await say(page, workspaceId, primed().id, probeMessage(token));
    assertPassed(classifyTerminalTurn(answered));
    expect(answered.response_text).toContain(probeReply(token));
    const chat = page.getByLabel(CHAT_LABEL);
    await expect(chat.getByText(probeReply(token)).first()).toBeVisible({timeout: RENDER_TIMEOUT_MS});
    await expect(chat.getByText(WAKE_TITLE).first()).toBeVisible({timeout: RENDER_TIMEOUT_MS});
  });

  test(proof(2), async ({page}) => {
    await signInAs(page, PRIME_PERSON);
    const asked = await say(page, workspaceId, primed().id, `${entry(fixture, fixture.follow_ups, 1, "follow-up")} ` +
      `Call ${TRIGGER_ADD} once with kind "webhook", source "${webhook.source}", secret_name "${webhook.secret_name}" and verifier ${JSON.stringify(VERIFIER)}.`);
    assertPassed(classifyTerminalTurn(asked));
    expect(toolsCalled(asked)).toContain(TRIGGER_ADD);
    const triggers = TriggersSchema.parse(await clientFor(PRIME_PERSON).get<unknown>(`${fleetPath()}/triggers`)).items;
    expect(triggers.map((row) => [row.kind, row.source])).toEqual([[TRIGGER_KIND.webhook, webhook.source]]);
    const url = triggers[0]?.url;
    if (!url) throw new Error("the webhook trigger names no address to deliver to");

    const body = JSON.stringify({sender: webhook.source, delivery: uniqueTag()});
    const first = await deliver(url, body, signature(secret, body));
    expect(first.status, JSON.stringify(first.answer)).toBe(HTTP_ACCEPTED);
    const accepted = AcceptedSchema.parse(first.answer);
    expect(accepted.replayed).toBe(false);
    delivered = {url, body, turnId: accepted.turn_id};
    const woken = await settledTurn(workspaceId, primed().id, (turn) => turn.turn_id === accepted.turn_id, RUN_TIMEOUT_MS);
    if (woken === null) throw new Error(`delivery ${accepted.turn_id} was accepted and its turn never settled`);
    assertPassed(classifyTerminalTurn(woken));
    expect(woken.actor).toBe(`${TURN_KIND_WEBHOOK}:${webhook.source}`);
    expect((await webhookTurns()).map((turn) => turn.turn_id)).toEqual([accepted.turn_id]);
  });

  test(proof(3), async () => {
    if (delivered === null) throw new Error("no delivery was accepted; nothing to replay");
    const {url, body, turnId} = delivered;
    const replay = await deliver(url, body, signature(secret, body));
    expect(replay.status, JSON.stringify(replay.answer)).toBe(HTTP_ACCEPTED);
    expect(AcceptedSchema.parse(replay.answer)).toEqual({turn_id: turnId, replayed: true});

    // A new body under another secret's signature: a different delivery, so only the signature can refuse it.
    const forged = JSON.stringify({sender: webhook.source, delivery: uniqueTag()});
    const refused = await deliver(url, forged, signature(crypto.randomBytes(24).toString("hex"), forged));
    expect(refused.status, JSON.stringify(refused.answer)).toBe(HTTP_UNAUTHORIZED);
    expect(RefusedSchema.parse(refused.answer).error_code).toBe(SIGNATURE_INVALID);

    // Both had their chance to become a turn; the one webhook turn is still the first delivery's.
    await new Promise((resolve) => setTimeout(resolve, REPLAY_GRACE_MS));
    expect((await webhookTurns()).map((turn) => turn.turn_id)).toEqual([turnId]);
  });
});
