/**
 * fleet-webhook-delivery.spec.ts — one signed GitHub delivery wakes a fleet
 * exactly once, and its exact replay wakes nothing.
 *
 * Wire: a workspace secret is stored → a fleet is asked, in its first message,
 * to add a webhook trigger that verifies GitHub's body signature with that
 * secret → its trigger row names the URL → a captured GitHub `workflow_run`
 * delivery is signed with the secret and posted to that URL → the
 * daemon accepts it and names the event → the identical bytes are posted
 * again → the daemon reports a replay and names the SAME event → an online
 * runner runs the one event, and the fleet's history carries exactly one
 * webhook row after both posts.
 *
 * The replay is byte-identical on purpose. The daemon keys its turn on the
 * digest of the signed body, never on the delivery header, so a resender
 * cannot mint a fresh turn by varying a header GitHub does not sign. The
 * fleet-authored REVIEW half of this proof needs a real repository and the
 * GitHub App installed on it, which is a live pass beside this journey, not
 * a fixture this one can arrange.
 */
import * as crypto from "node:crypto";
import * as fs from "node:fs";
import * as path from "node:path";
import { expect, test } from "@playwright/test";
import type { TurnsPage } from "@/lib/api/turns";
import { z } from "zod";
import { TRIGGER_KIND, TriggerSchema } from "@/lib/api/fleet-detail";
import { createFixtureFleet } from "./fixtures/fleet-create";
import { clientFor } from "./fixtures/api-client";
import { FIXTURE_KEY } from "./fixtures/constants";
import {
  anyRunnerLive,
  classifyIdle,
  TURN_STATUS,
  failWith,
} from "./fixtures/execution";
import { pollFor } from "./fixtures/observation";
import { getDefaultWorkspaceId, expectFleetActive } from "./fixtures/seed";
import { cleanWorkspaceFleets } from "./fixtures/teardown";

const FLEET_PREFIX = "fleet-webhook-";
// The workspace secret the trigger names. Not `github`: that name is the
// GitHub CONNECTION's handle, and a fleet secret must never shadow it.
//
// CONSTANT, and deliberately: secrets upsert by name, so rotating only the
// VALUE gives each run a secret no stale fleet can verify against, while the
// message asking for the trigger stays the same words.
const CREDENTIAL_NAME = "acceptance-webhook-secret";
const WEBHOOK_SECRET_FIELD = "webhook_secret";
// The trigger's source slug. Not `github`: a webhook may not take a reserved
// source, and an App subscription needs a real installation this fixture
// cannot arrange. GitHub's `sha256=<hex>` body signature is the generic
// `body-hex-sha256` recipe on its own header.
const WEBHOOK_SOURCE = "deploy-alerts";
const VERIFIER = {scheme: "body-hex-sha256", header: "x-hub-signature-256"} as const;
const WEBHOOK_EVENT_WORKFLOW_RUN = "workflow_run";

// The delivery GitHub sent for a failed deploy, captured once and reused by
// the daemon's own router suite. A failed `workflow_run` is the one shape the
// GitHub ingress accepts unconditionally.
const CAPTURED_DELIVERY = path.join(
  process.cwd(),
  "..",
  "..",
  "..",
  "rustd",
  "crates",
  "daemon",
  "afd_api_ingress",
  "tests",
  "fixtures",
  "webhooks",
  "github_run_failure.json",
);

// GitHub's headers, as the daemon reads them.
const HEADER_EVENT = "x-github-event";
const HEADER_DELIVERY = "x-github-delivery";
const HEADER_SIGNATURE = "x-hub-signature-256";
const SIGNATURE_PREFIX = "sha256=";
const HMAC_ALGORITHM = "sha256";

const TURN_KIND_WEBHOOK = "webhook";
const HTTP_ACCEPTED = 202;
// Delivery → run → durable row rides the queue and the heartbeat cadence,
// the same budget the other run-crossing journeys carry.
const TURN_ROW_TIMEOUT_MS = 120_000;
const POLL_INTERVAL_MS = 2_000;
// Long enough for a second delivery to have become a second row if the
// replay had not been suppressed; short enough not to be a wait. Derived
// from the poll cadence rather than picked, so it moves with it instead of
// silently becoming too short when the cadence slows.
const REPLAY_GRACE_POLLS = 5;
const REPLAY_GRACE_MS = POLL_INTERVAL_MS * REPLAY_GRACE_POLLS;
// The first message's run reaches a runner and calls trigger_add: the queue plus one provider round trip.
const TRIGGER_TIMEOUT_MS = 180_000;
const JOURNEY_TIMEOUT_MS = 420_000;
const TriggersSchema = z.object({items: z.array(TriggerSchema)});

interface Accepted {
  turn_id: string;
  replayed: boolean;
}

function sign(secret: string, body: Uint8Array): string {
  const digest = crypto.createHmac(HMAC_ALGORITHM, secret).update(body).digest("hex");
  return `${SIGNATURE_PREFIX}${digest}`;
}

// The webhook route is unauthenticated by design — GitHub holds no bearer —
// so this is a plain fetch, not the fixture client.
async function deliver(webhookUrl: string, body: Uint8Array, secret: string, delivery: string) {
  const response = await fetch(webhookUrl, {
    method: "POST",
    headers: {
      "content-type": "application/json",
      [HEADER_EVENT]: WEBHOOK_EVENT_WORKFLOW_RUN,
      [HEADER_DELIVERY]: delivery,
      [HEADER_SIGNATURE]: sign(secret, body),
    },
    // A fresh copy: fetch takes a `BufferSource`, and a Node `Buffer` is one
    // only once its backing store is not shared.
    body: new Uint8Array(body),
  });
  const text = await response.text();
  return { status: response.status, body: JSON.parse(text) as Accepted };
}

async function webhookTurns(workspaceId: string, fleetId: string) {
  const page = await clientFor(FIXTURE_KEY.regular).get<TurnsPage>(
    `/v1/workspaces/${workspaceId}/fleets/${fleetId}/turns`,
  );
  return page.items.filter((item) => item.event_type === TURN_KIND_WEBHOOK);
}

/** What the fleet is asked to do with its first message: add the one trigger this journey delivers to. */
function triggerMessage(): string {
  return `Call trigger_add once with kind "webhook", source "${WEBHOOK_SOURCE}", events ["${WEBHOOK_EVENT_WORKFLOW_RUN}"], ` +
    `verifier ${JSON.stringify(VERIFIER)} and secret_name "${CREDENTIAL_NAME}". Then reply with the single word done.`;
}

/** The webhook row the fleet added, once it names the URL deliveries go to. */
async function webhookTrigger(workspaceId: string, fleetId: string) {
  const page = TriggersSchema.parse(await clientFor(FIXTURE_KEY.regular)
    .get<unknown>(`/v1/workspaces/${workspaceId}/fleets/${fleetId}/triggers`));
  return page.items.find((item) => item.kind === TRIGGER_KIND.webhook && item.source === WEBHOOK_SOURCE && item.url) ?? null;
}

/** The workspace signing secret, and a fleet that added the trigger a delivery needs. */
async function arrangeWebhookFleet(workspaceId: string, tag: string) {
  const tenant = clientFor(FIXTURE_KEY.regular);
  // The VALUE is random per run, so a stale row from an interrupted run can
  // never verify this run's delivery; the NAME is not — see CREDENTIAL_NAME.
  const secret = crypto.randomBytes(24).toString("hex");
  await tenant.post(`/v1/workspaces/${workspaceId}/secrets`, {
    name: CREDENTIAL_NAME,
    data: { [WEBHOOK_SECRET_FIELD]: secret },
  });

  const { fleet } = await createFixtureFleet(FIXTURE_KEY.regular, workspaceId,
    { name: `${FLEET_PREFIX}${tag}`, message: triggerMessage() });
  await expectFleetActive(FIXTURE_KEY.regular, workspaceId, fleet.id);
  const webhook = await pollFor(() => webhookTrigger(workspaceId, fleet.id), TRIGGER_TIMEOUT_MS);
  if (webhook?.url == null) failWith(classifyIdle(await anyRunnerLive()));
  return { fleetId: fleet.id, webhookUrl: webhook.url, secret };
}

/**
 * Waits for a runner to take the event, or reports why nobody did.
 *
 * NO row exists until a runner runs the event. The ingress writes nothing to
 * the history — it admits the delivery as a queued turn and answers
 * 202, and the row is
 * written by the run (`afd_ingress::deliver`, module note: "the row appears
 * when the runner runs the event, and a daemon that inserted one at ingress
 * would be racing its own runner to describe the same event").
 *
 * This function read the opposite until Sep 8, 2026, and the cost was the
 * whole assertion: an absent row answered `null`, `null` is not `received`, so
 * the wait passed at once on a delivery nobody had taken and the journey went
 * on to fail on a bare length mismatch that named neither side. An empty
 * listing is the WAITING state, which is why it reads as `received` here — and
 * a wait that runs out reaches the classifier below, which is the sentence
 * this journey exists to print (RULE ECL).
 */
async function awaitRun(workspaceId: string, fleetId: string): Promise<void> {
  const held = await expect
    .poll(
      async () => {
        const [current] = await webhookTurns(workspaceId, fleetId);
        return current?.status ?? TURN_STATUS.received;
      },
      { timeout: TURN_ROW_TIMEOUT_MS, intervals: [POLL_INTERVAL_MS] },
    )
    .not.toBe(TURN_STATUS.received)
    .then(() => true)
    .catch(() => false);
  if (!held) {
    // A delivery nobody held is the runner fleet's fault when none is live
    // and the product's when one is — the same split every other
    // run-crossing journey reports, rather than a bare timeout.
    failWith(classifyIdle(await anyRunnerLive()));
  }
}

test.describe("fleet webhook delivery", () => {
  test.afterEach(async () => {
    const ws = await getDefaultWorkspaceId(FIXTURE_KEY.regular);
    await cleanWorkspaceFleets(FIXTURE_KEY.regular, ws, FLEET_PREFIX);
    await clientFor(FIXTURE_KEY.regular)
      .delete(`/v1/workspaces/${ws}/secrets/${encodeURIComponent(CREDENTIAL_NAME)}`)
      .catch(() => undefined);
  });

  test("a signed delivery wakes the fleet once and its replay wakes nothing", async () => {
    test.setTimeout(JOURNEY_TIMEOUT_MS);

    const tag = crypto.randomUUID().slice(0, 8);
    const workspaceId = await getDefaultWorkspaceId(FIXTURE_KEY.regular);
    const { fleetId, webhookUrl, secret } = await arrangeWebhookFleet(workspaceId, tag);

    // ── One delivery, accepted and named ──
    const body = new Uint8Array(fs.readFileSync(CAPTURED_DELIVERY));
    const delivery = crypto.randomUUID();
    const first = await deliver(webhookUrl, body, secret, delivery);
    expect(first.status, JSON.stringify(first.body)).toBe(HTTP_ACCEPTED);
    expect(first.body.replayed).toBe(false);
    expect(first.body.turn_id.length).toBeGreaterThan(0);

    // ── The exact replay, reported as one and naming the same event ──
    const replay = await deliver(webhookUrl, body, secret, delivery);
    expect(replay.status, JSON.stringify(replay.body)).toBe(HTTP_ACCEPTED);
    expect(replay.body.replayed).toBe(true);
    expect(replay.body.turn_id).toBe(first.body.turn_id);

    // ── One durable event, once a runner has taken it ──
    await awaitRun(workspaceId, fleetId);

    // The replay had its chance to become a second row across the grace above
    // plus the whole run wait; a count taken now is what proves it did not.
    await new Promise((resolve) => setTimeout(resolve, REPLAY_GRACE_MS));
    const rows = await webhookTurns(workspaceId, fleetId);
    expect(rows, "the replay added no second event").toHaveLength(1);
    const [row] = rows;
    expect(row?.turn_id).toBe(first.body.turn_id);
    expect(row?.status).not.toBe(TURN_STATUS.gateBlocked);
  });
});
