/**
 * A declared connector needs explicit approval before the fleet activates.
 * The CLI saves partial setup, answers its own approval, continues the exact
 * saved operation, runs a real message, observes spending, and revokes access.
 * The spending is read the way a terminal user reads it: `usage show` prints
 * the run's charge as a row, and its cursor pages back without repeating one.
 */

import { describe, it, beforeAll, afterAll } from "bun:test";
import assert from "node:assert/strict";
import fs from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import { Schema } from "effect";

import { ACCEPTANCE_TARGET_ENV } from "./fixtures/constants.ts";
import { composeEnv, runFleetctl } from "./fixtures/cli.js";
import type { RunResult } from "./fixtures/cli.js";
import { assertNoSecretLeak, refusalCode } from "./fixtures/negatives.ts";
import { trailingJson } from "./fixtures/steer-envelope.ts";
import {
  resolveAcceptanceEnv,
  resolveClerkSecret,
  resolveFixtureEmail,
} from "./global-setup.ts";
import { attachJwt } from "./fixtures/clerk-admin.ts";
import { hydrateWorkspacesForToken } from "./fixtures/workspace-hydration.ts";
import { addWorkspace, listWorkspaces, useWorkspace } from "./fixtures/workspace-ops.ts";
import { createNativeDraft, fixtureDefinition, inspectDraft } from "./fixtures/native-setup.ts";
import { FleetSchema } from "../../src/commands/chat/protocol.ts";
import { deleteFixtureFleet } from "./fixtures/teardown.ts";
import { readAuthContext, type AuthContext } from "./fixtures/native-api.ts";
import { chargedNanos, readUsagePage, usageTextNaming } from "./fixtures/usage-ops.ts";
import { EXIT_CODE } from "../../src/errors/index.ts";
import { instantMs } from "../../src/lib/instant.ts";
import {
  CONNECTOR_SERVICE_LINEAR,
  ensureConnectorHandle,
  GRANT_CREDENTIAL_NAME,
  GRANT_STATUS,
  KIND_INTEGRATION_GRANT,
  listGrants,
  pendingGateFor,
  REASON_DECLARED_AT_INSTALL,
  type GrantRow,
} from "./fixtures/grant-ops.ts";

const target = process.env[ACCEPTANCE_TARGET_ENV] ?? "";
const isLive = target.startsWith("https://");

const STATE_DIR_PREFIX = "agentsfleet-grant-" as const;
const GRANT_WORKSPACE = "fixture-cli-connector-grants";
const FLEET_NAME = "linear-issue-triage";
const NO_COLOR = "1" as const;
const JSON_FLAG = "--json" as const;
const STEER_COMMAND = ["fleet", "steer"] as const;
const BILLING_ARGS = ["usage", "show", JSON_FLAG] as const;
const BALANCE_FIELD = "balance_nanos" as const;
const ONE_SHOT_MESSAGE = "respond with a single short acknowledgement and stop" as const;
const ENVELOPE_STATUS_KEY = "status" as const;
const ENVELOPE_EVENT_ID_KEY = "event_id" as const;
const STATUS_PROCESSED = "processed" as const;
const GRANT_COMMAND = ["fleet", "grants"] as const;
const REVOKE_VERB = "revoke" as const;
const FLEET_FLAG = "--fleet" as const;
const CHAT_COMMAND = "chat";
const SAVE_FLAG = "--save";
const NO_INPUT_FLAG = "--no-input";
const REVIEW_DIGEST_FLAG = "--review-digest";
const RECOVERY_KIND = "setup_operation_recovery";
const recoverySchema = Schema.Struct({kind: Schema.Literal(RECOVERY_KIND), operation_id: Schema.String,
  draft_candidate_id: Schema.String, name: Schema.String, review_digest: Schema.String});

// A steer's SSE round trip falls back to a ~60s poll window before rendering;
// the budget has to exceed the CLI's own internal cap.
const STEER_TIMEOUT_MS = 180_000;
// Install plus the wait for the install-time grant row.
const SETUP_TIMEOUT_MS = 180_000;
// The usage ledger settles after the terminal row; the balance follows it.
const BALANCE_SETTLE_MS = 90_000;
const BALANCE_POLL_MS = 3_000;
// A dollar figure as `formatDollars` (src/constants/billing.ts) renders one.
const DOLLARS = /-?\$[\d,]+\.\d{2,}/u;
// Decodes to no boundary the charges route can read, so no page issued it
// (`afd_billing::tenant::cursor::parse`).
const FOREIGN_CURSOR = "not-a-cursor" as const;
// The charges route refuses a cursor it cannot parse as INVALID_REQUEST
// (afd_billing/src/error/mod.rs).
const CODE_INVALID_REQUEST = "UZ-REQ-001" as const;

if (!isLive) {
  describe("grant-approval-live.spec.ts", () => {
    it.skip(`requires ${ACCEPTANCE_TARGET_ENV} to be an https URL`, () => {});
  });
} else {
  describe("grant-approval-live — approve access, continue setup, run, and revoke", () => {
    let sessionJwt = "";
    let stateDir = "";
    let env: Record<string, string> = {};
    let workspaceId = "";
    let fleetId = "";
    let ctx: AuthContext | null = null;
    let installGrant: GrantRow | null = null;
    let recovery: typeof recoverySchema.Type | null = null;
    let balanceBeforeNanos = 0n;
    let steeredEventId = "";

    async function runWithEnv(args: ReadonlyArray<string>): Promise<RunResult> {
      const result = await runFleetctl(args, { env, timeoutMs: STEER_TIMEOUT_MS });
      assertNoSecretLeak(result, sessionJwt);
      return result;
    }

    async function readBalanceNanos(): Promise<bigint> {
      const result = await runWithEnv([...BILLING_ARGS]);
      assert.equal(result.code, 0, `billing show exited ${result.code}: ${result.stderr}`);
      const parsed = JSON.parse(result.stdout.trim()) as Record<string, unknown>;
      const balance = parsed[BALANCE_FIELD];
      assert.equal(typeof balance, "string", `billing show carried no ${BALANCE_FIELD}`);
      assert.match(String(balance), /^(0|[1-9][0-9]*)$/);
      return BigInt(String(balance));
    }

    beforeAll(async () => {
      const apiUrl = resolveAcceptanceEnv().apiUrl;
      const minted = await attachJwt(resolveClerkSecret(), {
        email: resolveFixtureEmail("regular"),
      });
      sessionJwt = minted.sessionJwt;

      stateDir = await fs.mkdtemp(path.join(os.tmpdir(), STATE_DIR_PREFIX));
      env = composeEnv({
        AGENTSFLEET_API_URL: apiUrl,
        AGENTSFLEET_STATE_DIR: stateDir,
        NO_COLOR,
      });
      workspaceId = (await hydrateWorkspacesForToken({ apiUrl, token: sessionJwt, stateDir }))
        .currentWorkspaceId;
      const existing = (await listWorkspaces(env)).find(workspace => workspace.name === GRANT_WORKSPACE);
      workspaceId = existing?.workspace_id ?? (await addWorkspace(env, GRANT_WORKSPACE)).workspaceId;
      await useWorkspace(env, workspaceId);
      ctx = await readAuthContext(env);

      balanceBeforeNanos = await readBalanceNanos();
      await ensureConnectorHandle(env);
      const draft = await createNativeDraft(ctx, fixtureDefinition(FLEET_NAME, GRANT_CREDENTIAL_NAME));
      const review = await inspectDraft(env, draft.id);
      const saved = await runWithEnv([JSON_FLAG, CHAT_COMMAND, draft.id, NO_INPUT_FLAG, SAVE_FLAG,
        REVIEW_DIGEST_FLAG, review.review_digest]);
      assert.equal(saved.code, 1, `unapproved setup must remain partial: ${saved.stderr}`);
      const fleet = saved.stdout.trim().split("\n").map((line): unknown => JSON.parse(line)).find(Schema.is(FleetSchema));
      assert.ok(fleet, "Save returned no fleet");
      fleetId = fleet.id;
      assert.equal(fleet.status, "installing");
      recovery = Schema.decodeUnknownSync(recoverySchema)(trailingJson(saved.stderr));
      installGrant = (await listGrants(env, fleetId)).find((row) => row.service === CONNECTOR_SERVICE_LINEAR) ?? null;
    }, SETUP_TIMEOUT_MS);

    afterAll(async () => {
      try {
        if (workspaceId) {
          if (fleetId) await deleteFixtureFleet(env, fleetId);
          const removed = await runFleetctl(["secrets", "delete", GRANT_CREDENTIAL_NAME, JSON_FLAG], {env});
          assert.equal(removed.code, 0, `synthetic connector cleanup failed: ${removed.stderr}`);
        }
      } finally {
        if (stateDir) await fs.rm(stateDir, { recursive: true, force: true });
      }
    });

    it("a declared connector leaves a pending grant and keeps setup from activating", async () => {
      assert.ok(fleetId, "the fleet was not installed in beforeAll");
      const grants = await listGrants(env, fleetId);
      const mine = grants.filter((row) => row.service === CONNECTOR_SERVICE_LINEAR);
      assert.equal(mine.length, 1,
        `expected one ${CONNECTOR_SERVICE_LINEAR} grant for the fleet; got ${JSON.stringify(grants)}`);
      const grant = mine[0];
      assert.equal(grant?.status, GRANT_STATUS.pending,
        `saving is not consent: ${JSON.stringify(grant)}`);
      assert.equal(grant?.approved_at ?? null, null,
        `a pending grant cannot carry approved_at: ${JSON.stringify(grant)}`);
      assert.equal(grant?.revoked_at ?? null, null,
        `a fresh install-time grant is not revoked: ${JSON.stringify(grant)}`);
      // The provenance separates this row from one a person answered: only the
      // install writer stamps this sentence.
      assert.equal(grant?.reason, REASON_DECLARED_AT_INSTALL,
        `the grant must name the install as its provenance: ${JSON.stringify(grant)}`);
    });

    it("the CLI inbox shows the pending access request for this fleet", async () => {
      assert.ok(installGrant, "Save wrote no grant");
      assert.ok(ctx, "no auth context");
      const card = await pendingGateFor(ctx, fleetId);
      assert.ok(card, "Save must request explicit integration approval");
      const listed = await runFleetctl(
        ["fleet", "approvals", "list", FLEET_FLAG, fleetId, JSON_FLAG],
        { env },
      );
      assert.equal(listed.code, 0, `approvals list failed: ${listed.stderr}`);
      const gates =
        (trailingJson(listed.stdout) as { items?: Array<{ gate_type?: string }> }).items ?? [];
      const cards = gates.filter((row) => row.gate_type === KIND_INTEGRATION_GRANT);
      assert.equal(cards.length, 1,
        `the inbox must carry this fleet's ${KIND_INTEGRATION_GRANT} request: ${listed.stdout}`);
    });

    it("explicit approval and continuation activate the same saved fleet", async () => {
      assert.ok(ctx, "no auth context");
      assert.ok(recovery, "no continuation identity");
      const card = await pendingGateFor(ctx, fleetId);
      assert.ok(card, "no approval request");
      const approved = await runWithEnv(["fleet", "approvals", "approve", card.gate_id, JSON_FLAG]);
      assert.equal(approved.code, 0, `approval failed: ${approved.stderr}`);
      const continued = await runWithEnv([JSON_FLAG, CHAT_COMMAND, NO_INPUT_FLAG, SAVE_FLAG,
        "--operation-id", recovery.operation_id, "--candidate-id", recovery.draft_candidate_id,
        "--name", recovery.name, REVIEW_DIGEST_FLAG, recovery.review_digest]);
      assert.equal(continued.code, 0, `continuation failed: ${continued.stderr}`);
      const fleet = continued.stdout.trim().split("\n").map((line): unknown => JSON.parse(line)).find(Schema.is(FleetSchema));
      assert.ok(fleet, "continuation returned no fleet");
      assert.equal(fleet.id, fleetId, "continuation must retain the saved identity");
      assert.equal(fleet.status, "active");
      const grant = (await listGrants(env, fleetId)).find((row) => row.service === CONNECTOR_SERVICE_LINEAR);
      assert.equal(grant?.status, GRANT_STATUS.approved);
      assert.ok(grant?.approved_at);
      assert.equal(await pendingGateFor(ctx, fleetId), null);
    });

    it("the granted fleet answers a steer, and the tenant balance falls", async () => {
      assert.ok(fleetId, "the fleet was not installed in beforeAll");
      const result = await runWithEnv([...STEER_COMMAND, fleetId, ONE_SHOT_MESSAGE, JSON_FLAG]);
      assert.equal(result.code, 0,
        `a granted fleet's steer must exit 0; stdout=${result.stdout} stderr=${result.stderr}`);
      const envelope = trailingJson(result.stdout) as Record<string, unknown>;
      assert.equal(envelope[ENVELOPE_STATUS_KEY], STATUS_PROCESSED,
        `expected ${STATUS_PROCESSED}; got ${JSON.stringify(envelope)}`);
      const eventId = envelope[ENVELOPE_EVENT_ID_KEY];
      assert.equal(typeof eventId, "string", `the steer named no work: ${JSON.stringify(envelope)}`);
      steeredEventId = String(eventId);

      // Nanos, never a rendered figure: a run costing a fraction of a cent
      // moves the balance and no two-decimal display.
      const deadline = Date.now() + BALANCE_SETTLE_MS;
      let balanceAfterNanos = await readBalanceNanos();
      while (balanceAfterNanos >= balanceBeforeNanos && Date.now() < deadline) {
        await Bun.sleep(BALANCE_POLL_MS);
        balanceAfterNanos = await readBalanceNanos();
      }
      assert.ok(balanceAfterNanos < balanceBeforeNanos,
        `the run did not deplete credit: ${balanceBeforeNanos} → ${balanceAfterNanos} nanos`);
    }, STEER_TIMEOUT_MS);

    it("`usage show` prints the run's charge as a readable row, and the JSON charges this fleet", async () => {
      assert.ok(steeredEventId, "the steer did not run");
      const text = await usageTextNaming(runWithEnv, steeredEventId);
      assert.match(text, /Tenant balance:/u);
      assert.match(text, /activities drained credits:/u);
      // A table row or, on a narrow terminal, a label/value block: either way
      // the stretch of output naming the work also names its total in dollars.
      const row = text.split(/\n\s*\n/u).find((block) => block.includes(steeredEventId)) ?? "";
      assert.match(row, /TOTAL/u, `the run's row carries no total: ${text}`);
      assert.match(row, DOLLARS, `the run's row carries no dollar figure: ${text}`);

      const charged = await chargedNanos(runWithEnv, fleetId, steeredEventId);
      assert.ok(charged > 0n, `usage show charged the run nothing: ${charged} nanos`);
    }, STEER_TIMEOUT_MS);

    it("`usage show --cursor` continues to an older charge without repeating the first page's", async () => {
      assert.ok(steeredEventId, "the steer did not run");
      const first = await readUsagePage(runWithEnv, 1);
      assert.equal(first.events.length, 1, `a one-row page grouped into ${first.events.length} entries`);
      assert.ok(first.next_cursor, "a full page must offer the cursor that continues it");
      // A run writes a model row per request and the daemon adds a runtime
      // row (af_wire/src/usage/mod.rs), so the run alone fills a second page.
      const second = await readUsagePage(runWithEnv, 1, first.next_cursor);
      assert.equal(second.events.length, 1, "the page after a full one came back empty");
      // The daemon renders a cursor from its page's last row, so an equal
      // cursor means page two handed back page one's row.
      assert.notEqual(second.next_cursor, first.next_cursor, "page two repeated page one's row");
      const newer = instantMs(first.events[0]?.recorded_at);
      const older = instantMs(second.events[0]?.recorded_at);
      assert.ok(newer !== null && older !== null && older <= newer,
        `the walk went forward in time: ${JSON.stringify([first, second])}`);
    });

    it("`usage show --cursor` with a token no page issued is refused by the daemon", async () => {
      const refused = await runWithEnv(["usage", "show", "--cursor", FOREIGN_CURSOR, JSON_FLAG]);
      assert.equal(refused.code, EXIT_CODE.ServerError, `a foreign cursor was accepted: ${refused.stdout}`);
      assert.equal(refusalCode(refused), CODE_INVALID_REQUEST);
    });

    // Last, and deliberately so: revoking the standing grant is what stops the
    // fleet reaching the provider, so every step that needs it has run by here.
    it("`fleet grants revoke` revokes the standing permission the install wrote", async () => {
      const grantId = installGrant?.id;
      assert.ok(grantId, `the install-time grant carried no id: ${JSON.stringify(installGrant)}`);
      const revoked = await runWithEnv([...GRANT_COMMAND, REVOKE_VERB, grantId, FLEET_FLAG, fleetId]);
      assert.equal(revoked.code, 0, `fleet grants revoke exited ${revoked.code}: ${revoked.stderr}`);

      const grants = await listGrants(env, fleetId);
      const mine = grants.filter((row) => row.service === CONNECTOR_SERVICE_LINEAR);
      assert.equal(mine.length, 1,
        `the revoke must move the row, never add one: ${JSON.stringify(grants)}`);
      assert.equal(mine[0]?.status, GRANT_STATUS.revoked,
        `the CLI still reports the grant standing: ${JSON.stringify(mine[0])}`);
      assert.ok(mine[0]?.revoked_at,
        `a revoked grant must carry revoked_at: ${JSON.stringify(mine[0])}`);
    });
  });
}
