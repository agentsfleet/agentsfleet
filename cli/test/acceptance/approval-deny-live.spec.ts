/**
 * approval-deny-live — "no, don't do that", answered from the terminal.
 *
 * A fleet whose setup declares a connector cannot activate until a person
 * grants it that access: Save leaves the fleet installing behind an
 * `integration_grant` gate. This walk raises that gate itself, the way
 * grant-approval-live does, so the denial is never decided against whatever
 * happens to be sitting in a shared inbox, and then refuses it:
 *
 *   - `approvals show` reads the pending request
 *   - `approvals deny` answers denied, and the inbox holds it pending no more
 *   - the grant the install wrote is revoked, and the fleet stays installing
 *   - a second answer is refused with the decision that stands
 *
 * # Why no event or log carries the refusal
 *
 * An install-time gate holds no run. Its `turn_id` is null, which is what
 * `afd_approval/src/sql/resolve_gate.sql` keys the grant update on, and a
 * denial lands no continuation, so `fleet turns` has no row to show it in.
 * The refusal is visible where this gate's effect lives instead: the grant
 * row and the fleet's status. A run-time gate needs a trigger that declares a
 * write capability, which no fixture in this lane provides.
 *
 * Live-only: registers real tests only when `AGENTSFLEET_ACCEPTANCE_TARGET` is
 * an https URL, matching every other live spec in this lane.
 */

import { describe, it, beforeAll, afterAll } from "bun:test";
import assert from "node:assert/strict";
import fs from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import { Schema } from "effect";

import { ACCEPTANCE_TARGET_ENV, AGENTSFLEET_STATUS } from "./fixtures/constants.ts";
import { composeEnv, runFleetctl } from "./fixtures/cli.js";
import type { RunResult } from "./fixtures/cli.js";
import { assertNoSecretLeak, refusalCode } from "./fixtures/negatives.ts";
import { resolveAcceptanceEnv, resolveClerkSecret, resolveFixtureEmail } from "./global-setup.ts";
import { attachJwt } from "./fixtures/clerk-admin.ts";
import { hydrateWorkspacesForToken } from "./fixtures/workspace-hydration.ts";
import { addWorkspace, listWorkspaces, useWorkspace } from "./fixtures/workspace-ops.ts";
import { fixtureDefinition, saveNativeFleet } from "./fixtures/native-setup.ts";
import { readAuthContext, type AuthContext } from "./fixtures/native-api.ts";
import { deleteFixtureFleet } from "./fixtures/teardown.ts";
import { getStatus } from "./fixtures/lifecycle.ts";
import { randomUuidv7 } from "./helpers-lifecycle-with-token.ts";
import {
  CONNECTOR_SERVICE_LINEAR,
  ensureConnectorHandle,
  GRANT_CREDENTIAL_NAME,
  GRANT_STATUS,
  KIND_INTEGRATION_GRANT,
  listGrants,
  pendingGateFor,
} from "./fixtures/grant-ops.ts";
import { GATE_STATUS } from "../../src/constants/approvals.ts";
import { EXIT_CODE } from "../../src/errors/index.ts";

const target = process.env[ACCEPTANCE_TARGET_ENV] ?? "";
const isLive = target.startsWith("https://");

const STATE_DIR_PREFIX = "agentsfleet-deny-" as const;
// Its own workspace: the connector handle is a workspace secret under the
// connector's name, and grant-approval-live creates and deletes the same name
// in its workspace while this file runs beside it.
const DENIAL_WORKSPACE = "fixture-cli-approval-denials";
const FLEET_NAME = "linear-backlog-digest";
const NO_COLOR = "1" as const;
const JSON_FLAG = "--json" as const;
const APPROVALS = ["fleet", "approvals"] as const;
const SETUP_TIMEOUT_MS = 180_000;
// af_core/src/error_code/request.rs: a second answer and an unknown gate.
const CODE_ALREADY_RESOLVED = "UZ-APPROVAL-006" as const;
const CODE_GATE_NOT_FOUND = "UZ-APPROVAL-002" as const;

const GateSchema = Schema.Struct({
  gate_id: Schema.String,
  fleet_id: Schema.String,
  gate_type: Schema.String,
  status: Schema.String,
  blast_radius: Schema.String,
  resolved_at: Schema.NullOr(Schema.String),
  resolved_by: Schema.String,
});
const GatePageSchema = Schema.Struct({ items: Schema.Array(GateSchema) });
const DecisionSchema = Schema.Struct({
  gate_id: Schema.String,
  outcome: Schema.String,
  resolved_at: Schema.String,
  resolved_by: Schema.String,
});

if (!isLive) {
  describe("approval-deny-live.spec.ts", () => {
    it.skip(`requires ${ACCEPTANCE_TARGET_ENV} to be an https URL`, () => {});
  });
} else {
  describe("approval-deny-live — refuse a fleet's access request from the terminal", () => {
    let sessionJwt = "";
    let stateDir = "";
    let env: Record<string, string> = {};
    let workspaceId = "";
    let ctx: AuthContext | null = null;
    let fleetId = "";
    let savedStatus = "";
    let gateId = "";

    async function run(args: ReadonlyArray<string>): Promise<RunResult> {
      const result = await runFleetctl(args, { env });
      assertNoSecretLeak(result, sessionJwt);
      return result;
    }

    async function showGate(): Promise<typeof GateSchema.Type> {
      const result = await run([...APPROVALS, "show", gateId, JSON_FLAG]);
      assert.equal(result.code, 0, `approvals show failed: ${result.stderr}`);
      return Schema.decodeUnknownSync(GateSchema)(JSON.parse(result.stdout.trim()));
    }

    async function linearGrant() {
      return (await listGrants(env, fleetId)).find((row) => row.service === CONNECTOR_SERVICE_LINEAR);
    }

    beforeAll(async () => {
      const apiUrl = resolveAcceptanceEnv().apiUrl;
      const minted = await attachJwt(resolveClerkSecret(), { email: resolveFixtureEmail("regular") });
      sessionJwt = minted.sessionJwt;
      stateDir = await fs.mkdtemp(path.join(os.tmpdir(), STATE_DIR_PREFIX));
      env = composeEnv({ AGENTSFLEET_API_URL: apiUrl, AGENTSFLEET_STATE_DIR: stateDir, NO_COLOR });
      await hydrateWorkspacesForToken({ apiUrl, token: sessionJwt, stateDir });
      const existing = (await listWorkspaces(env)).find((workspace) => workspace.name === DENIAL_WORKSPACE);
      workspaceId = existing?.workspace_id ?? (await addWorkspace(env, DENIAL_WORKSPACE)).workspaceId;
      await useWorkspace(env, workspaceId);
      ctx = await readAuthContext(env);

      await ensureConnectorHandle(env);
      const fleet = await saveNativeFleet(env,
        fixtureDefinition(FLEET_NAME, GRANT_CREDENTIAL_NAME));
      fleetId = fleet.id;
      savedStatus = fleet.status;
    }, SETUP_TIMEOUT_MS);

    afterAll(async () => {
      try {
        if (workspaceId) {
          if (fleetId) await deleteFixtureFleet(env, fleetId);
          const removed = await runFleetctl(["secrets", "delete", GRANT_CREDENTIAL_NAME, JSON_FLAG], { env });
          assert.equal(removed.code, 0, `synthetic connector cleanup failed: ${removed.stderr}`);
        }
      } finally {
        if (stateDir) await fs.rm(stateDir, { recursive: true, force: true });
      }
    });

    it("Save leaves the fleet installing behind a pending access request", async () => {
      assert.ok(ctx, "no auth context");
      assert.equal(savedStatus, AGENTSFLEET_STATUS.installing, "an ungranted connector must keep setup partial");
      const card = await pendingGateFor(ctx, fleetId);
      assert.ok(card, "Save raised no access request for the declared connector");
      gateId = card.gate_id;

      const gate = await showGate();
      assert.equal(gate.fleet_id, fleetId);
      assert.equal(gate.gate_type, KIND_INTEGRATION_GRANT);
      assert.equal(gate.status, GATE_STATUS.pending);
      assert.equal(gate.resolved_at, null);
      assert.ok(gate.blast_radius.length > 0, "a request must carry the sentence the decision turns on");
    });

    it("`approvals deny` answers denied, and the inbox holds the request pending no more", async () => {
      assert.ok(gateId && ctx, "no access request was raised");
      const denied = await run([...APPROVALS, "deny", gateId, JSON_FLAG]);
      assert.equal(denied.code, 0, `approvals deny failed: ${denied.stderr}`);
      const decision = Schema.decodeUnknownSync(DecisionSchema)(JSON.parse(denied.stdout.trim()));
      assert.equal(decision.gate_id, gateId);
      assert.equal(decision.outcome, GATE_STATUS.denied);
      assert.ok(decision.resolved_by.length > 0, "a decision must name who made it");

      const gate = await showGate();
      assert.equal(gate.status, GATE_STATUS.denied);
      assert.equal(gate.resolved_at, decision.resolved_at);

      const listed = await run([...APPROVALS, "list", "--fleet", fleetId, JSON_FLAG]);
      assert.equal(listed.code, 0, `approvals list failed: ${listed.stderr}`);
      const rows = Schema.decodeUnknownSync(GatePageSchema)(JSON.parse(listed.stdout.trim())).items;
      assert.equal(rows.find((row) => row.gate_id === gateId)?.status, GATE_STATUS.denied);
      assert.ok(rows.every((row) => row.status !== GATE_STATUS.pending),
        `the fleet still has a request waiting: ${listed.stdout}`);
      assert.equal(await pendingGateFor(ctx, fleetId), null);
    });

    it("the denial revokes the grant the install wrote, and the fleet does not activate", async () => {
      assert.ok(gateId, "no access request was raised");
      const grant = await linearGrant();
      assert.equal(grant?.status, GRANT_STATUS.revoked, `a denied request left the grant: ${JSON.stringify(grant)}`);
      assert.ok(grant?.revoked_at, `a revoked grant must carry revoked_at: ${JSON.stringify(grant)}`);
      assert.equal(grant?.approved_at ?? null, null, "a denied grant was never approved");
      assert.equal((await getStatus(env, fleetId)).status, AGENTSFLEET_STATUS.installing,
        "a fleet whose access was refused must not run");
    });

    it("approving a denied request is refused with the decision that stands", async () => {
      assert.ok(gateId, "no access request was raised");
      const late = await run([...APPROVALS, "approve", gateId, JSON_FLAG]);
      assert.equal(late.code, EXIT_CODE.ServerError, `a decided request took a second answer: ${late.stdout}`);
      assert.equal(refusalCode(late), CODE_ALREADY_RESOLVED);
      assert.equal((await showGate()).status, GATE_STATUS.denied, "the late approval overturned the denial");
      assert.equal((await linearGrant())?.status, GRANT_STATUS.revoked);
    });

    it("`approvals deny` on a request this workspace does not hold is refused as unknown", async () => {
      const refused = await run([...APPROVALS, "deny", randomUuidv7(), JSON_FLAG]);
      assert.equal(refused.code, EXIT_CODE.ServerError, `an unknown request was decided: ${refused.stdout}`);
      assert.equal(refusalCode(refused), CODE_GATE_NOT_FOUND);
    });
  });
}
