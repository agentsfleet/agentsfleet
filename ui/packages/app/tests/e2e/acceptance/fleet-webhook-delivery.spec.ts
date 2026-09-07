/**
 * fleet-webhook-delivery.spec.ts — one signed GitHub delivery wakes a fleet
 * exactly once, and its exact replay wakes nothing.
 *
 * Wire: a fleet whose TRIGGER.md declares a GitHub `workflow_run` webhook and
 * names a workspace secret → that secret is stored → a captured GitHub
 * delivery is signed with it and posted to the fleet's webhook route → the
 * daemon accepts it and names the event → the identical bytes are posted
 * again → the daemon reports a replay and names the SAME event → an online
 * runner leases the one event, and the fleet's history carries exactly one
 * webhook row after both posts.
 *
 * The replay is byte-identical on purpose. The daemon keys its claim on the
 * digest of the signed body, never on the delivery header, so a resender
 * cannot mint a fresh claim by varying a header GitHub does not sign. The
 * fleet-authored REVIEW half of this proof needs a real repository and the
 * GitHub App installed on it, which is a live pass beside this journey, not
 * a fixture this one can arrange.
 */
import * as crypto from "node:crypto";
import * as fs from "node:fs";
import * as path from "node:path";
import { expect, test } from "@playwright/test";
import type { EventsPage } from "@/lib/api/events";
import { SOURCE_KIND_UPLOAD } from "@/lib/types";
import { clientFor } from "./fixtures/api-client";
import { FIXTURE_KEY } from "./fixtures/constants";
import {
  anyRunnerLive,
  classifyUnleased,
  EVENT_STATUS,
  failWith,
} from "./fixtures/execution";
import {
  executionSkillMd,
  getDefaultWorkspaceId,
  WEBHOOK_EVENT_WORKFLOW_RUN,
  waitForFleetActive,
  webhookTriggerMd,
} from "./fixtures/seed";
import { cleanWorkspaceFleets } from "./fixtures/teardown";

// The library converges on one row per body; the fleet is named per run.
const WEBHOOK_TEMPLATE = "fleet-webhook-delivery";
const FLEET_PREFIX = "fleet-webhook-";
// The workspace secret the trigger names. Not `github`: that name is the
// GitHub CONNECTION's handle, and a fleet secret must never shadow it.
//
// CONSTANT, and deliberately. The name goes into `trigger_markdown`, so a
// per-run name changed the bundle's BYTES and the library minted a fresh row
// every run — the accumulation `install-ui.ts` records reaching a hundred
// rows and pushing the platform catalogue off its first page. Secrets upsert
// by name, so rotating only the VALUE still gives each run a secret no stale
// row can verify against, while the bundle stays byte-identical.
const CREDENTIAL_NAME = "acceptance-webhook-secret";
const WEBHOOK_SECRET_FIELD = "webhook_secret";

// The delivery GitHub sent for a failed deploy, captured once and reused by
// the daemon's own router suite. A failed `workflow_run` is the one shape the
// GitHub ingress accepts unconditionally.
const CAPTURED_DELIVERY = path.join(
  process.cwd(),
  "..",
  "..",
  "..",
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

const EVENT_TYPE_WEBHOOK = "webhook";
const HTTP_ACCEPTED = 202;
// Delivery → lease → durable row rides the queue and the heartbeat cadence,
// the same budget the other lease-crossing journeys carry.
const EVENT_ROW_TIMEOUT_MS = 120_000;
const POLL_INTERVAL_MS = 2_000;
// Long enough for a second delivery to have become a second row if the
// replay had not been suppressed; short enough not to be a wait. Derived
// from the poll cadence rather than picked, so it moves with it instead of
// silently becoming too short when the cadence slows.
const REPLAY_GRACE_POLLS = 5;
const REPLAY_GRACE_MS = POLL_INTERVAL_MS * REPLAY_GRACE_POLLS;
const JOURNEY_TIMEOUT_MS = 240_000;

interface OnboardTemplateResp {
  id: string;
}

interface CreateFleetResp {
  fleet_id: string;
}

interface Accepted {
  event_id: string;
  replayed: boolean;
}

function apiBase(): string {
  const url = process.env.NEXT_PUBLIC_API_URL;
  if (!url) throw new Error("NEXT_PUBLIC_API_URL must be set");
  return url;
}

function sign(secret: string, body: Uint8Array): string {
  const digest = crypto.createHmac(HMAC_ALGORITHM, secret).update(body).digest("hex");
  return `${SIGNATURE_PREFIX}${digest}`;
}

// The webhook route is unauthenticated by design — GitHub holds no bearer —
// so this is a plain fetch, not the fixture client.
async function deliver(fleetId: string, body: Uint8Array, secret: string, delivery: string) {
  const response = await fetch(`${apiBase()}/v1/webhooks/${fleetId}/github`, {
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

async function webhookEvents(workspaceId: string, fleetId: string) {
  const page = await clientFor(FIXTURE_KEY.regular).get<EventsPage>(
    `/v1/workspaces/${workspaceId}/fleets/${fleetId}/events`,
  );
  return page.items.filter((item) => item.event_type === EVENT_TYPE_WEBHOOK);
}

/** The secret, the library row and the fleet a delivery needs to land on. */
async function arrangeWebhookFleet(workspaceId: string, tag: string) {
  const tenant = clientFor(FIXTURE_KEY.regular);
  // The VALUE is random per run, so a stale row from an interrupted run can
  // never verify this run's delivery; the NAME is not, because it rides the
  // bundle bytes — see CREDENTIAL_NAME.
  const secret = crypto.randomBytes(24).toString("hex");
  await tenant.post(`/v1/workspaces/${workspaceId}/secrets`, {
    name: CREDENTIAL_NAME,
    data: { [WEBHOOK_SECRET_FIELD]: secret },
  });

  const library = await tenant.post<OnboardTemplateResp>(
    `/v1/workspaces/${workspaceId}/fleet-libraries`,
    {
      source_kind: SOURCE_KIND_UPLOAD,
      skill_markdown: executionSkillMd(WEBHOOK_TEMPLATE),
      trigger_markdown: webhookTriggerMd(WEBHOOK_TEMPLATE, CREDENTIAL_NAME),
    },
  );
  const fleet = await tenant.post<CreateFleetResp>(`/v1/workspaces/${workspaceId}/fleets`, {
    tenant_library_id: library.id,
    name: `${FLEET_PREFIX}${tag}`,
  });
  await waitForFleetActive(FIXTURE_KEY.regular, workspaceId, fleet.fleet_id);
  return { fleetId: fleet.fleet_id, secret };
}

/**
 * Waits for a runner to take the event, or reports why nobody did.
 *
 * The row appears synchronously — the ingress appends it before answering 202
 * — so its EXISTENCE proves only that the POST returned. What proves a runner
 * took it is the status leaving `received`.
 */
async function awaitLease(workspaceId: string, fleetId: string): Promise<void> {
  const leased = await expect
    .poll(
      async () => {
        const [current] = await webhookEvents(workspaceId, fleetId);
        return current?.status ?? null;
      },
      { timeout: EVENT_ROW_TIMEOUT_MS, intervals: [POLL_INTERVAL_MS] },
    )
    .not.toBe(EVENT_STATUS.received)
    .then(() => true)
    .catch(() => false);
  if (!leased) {
    // A delivery nobody leased is the runner fleet's fault when none is live
    // and the product's when one is — the same split every other
    // lease-crossing journey reports, rather than a bare timeout.
    failWith(classifyUnleased(await anyRunnerLive()));
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
    const { fleetId, secret } = await arrangeWebhookFleet(workspaceId, tag);

    // ── One delivery, accepted and named ──
    const body = new Uint8Array(fs.readFileSync(CAPTURED_DELIVERY));
    const delivery = crypto.randomUUID();
    const first = await deliver(fleetId, body, secret, delivery);
    expect(first.status, JSON.stringify(first.body)).toBe(HTTP_ACCEPTED);
    expect(first.body.replayed).toBe(false);
    expect(first.body.event_id.length).toBeGreaterThan(0);

    // ── The exact replay, reported as one and naming the same event ──
    const replay = await deliver(fleetId, body, secret, delivery);
    expect(replay.status, JSON.stringify(replay.body)).toBe(HTTP_ACCEPTED);
    expect(replay.body.replayed).toBe(true);
    expect(replay.body.event_id).toBe(first.body.event_id);

    // ── One durable event, once a runner has taken it ──
    await awaitLease(workspaceId, fleetId);

    // The replay had its chance to become a second row across the grace above
    // plus the whole lease wait; a count taken now is what proves it did not.
    await new Promise((resolve) => setTimeout(resolve, REPLAY_GRACE_MS));
    const rows = await webhookEvents(workspaceId, fleetId);
    expect(rows, "the replay added no second event").toHaveLength(1);
    const [row] = rows;
    expect(row?.event_id).toBe(first.body.event_id);
    expect(row?.status).not.toBe(EVENT_STATUS.gateBlocked);
  });
});
