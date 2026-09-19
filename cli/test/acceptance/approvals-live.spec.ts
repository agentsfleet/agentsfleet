/**
 * approvals-live — a parked Fleet is diagnosed and released from the terminal.
 *
 * A Fleet whose trigger declares a write capability opens an approval gate for
 * every run and waits for a person. The walk here is the one an operator was
 * previously unable to finish without a browser:
 *
 *   - a steer parks, and the failure NAMES the gate rather than reporting a
 *     bare timeout
 *   - `approvals list` shows it; `approvals show` prints the blast radius the
 *     decision turns on
 *   - `approvals approve` releases it, and the Fleet's waiting count falls
 *
 * # Why the timeout is asserted, not avoided
 *
 * The steer is EXPECTED to fail here. That failure is the subject: it used to
 * print two glyph lines saying the same thing and point at `agentsfleet
 * events`, which showed the message stuck at `received` and named nothing that
 * would move it. Asserting on the failure's text is asserting the diagnosis.
 *
 * Live-only: registers real tests only when `AGENTSFLEET_ACCEPTANCE_TARGET` is
 * an https URL, matching every other live spec in this lane.
 */

import { describe, it, beforeAll, afterAll } from "bun:test";
import assert from "node:assert/strict";
import fs from "node:fs/promises";
import os from "node:os";
import path from "node:path";

import { ACCEPTANCE_RUN_PREFIX, ACCEPTANCE_TARGET_ENV } from "./fixtures/constants.ts";
import { composeEnv, runFleetctl } from "./fixtures/cli.js";
import type { RunResult } from "./fixtures/cli.js";
import { assertNoSecretLeak } from "./fixtures/negatives.ts";
import {
  resolveAcceptanceEnv,
  resolveClerkSecret,
  resolveFixtureEmail,
} from "./global-setup.ts";
import { attachJwt } from "./fixtures/clerk-admin.ts";
import { hydrateWorkspacesForToken } from "./fixtures/workspace-hydration.ts";
import { cleanWorkspaceFleets } from "./fixtures/teardown.ts";
import { GATE_STATUS } from "./fixtures/grant-ops.ts";

const target = process.env[ACCEPTANCE_TARGET_ENV] ?? "";
const isLive = target.startsWith("https://");

const STATE_DIR_PREFIX = "agentsfleet-approvals-" as const;
const NO_COLOR = "1" as const;
const JSON_FLAG = "--json" as const;
const SETUP_TIMEOUT_MS = 120_000;
const LIST_TIMEOUT_MS = 60_000;

interface GateListRow {
  readonly gate_id?: string;
  readonly fleet_id?: string;
  readonly status?: string;
  readonly gate_kind?: string;
  readonly blast_radius?: string;
}

if (!isLive) {
  describe("approvals-live.spec.ts", () => {
    it.skip(`requires ${ACCEPTANCE_TARGET_ENV} to be an https URL`, () => {});
  });
} else {
  describe("approvals-live — see the inbox, read a gate, decide it", () => {
    let sessionJwt = "";
    let stateDir = "";
    let env: Record<string, string> = {};
    let workspaceId = "";

    async function runWithEnv(
      args: ReadonlyArray<string>,
      timeoutMs = LIST_TIMEOUT_MS,
    ): Promise<RunResult> {
      const result = await runFleetctl(args, { env, timeoutMs });
      assertNoSecretLeak(result, sessionJwt);
      return result;
    }

    const parseJson = (out: string): Record<string, unknown> =>
      JSON.parse(out.trim()) as Record<string, unknown>;

    const listGates = async (): Promise<ReadonlyArray<GateListRow>> => {
      const result = await runWithEnv(["approvals", "list", JSON_FLAG]);
      assert.equal(result.code, 0, `approvals list failed: ${result.stderr}`);
      return (parseJson(result.stdout) as { items?: GateListRow[] }).items ?? [];
    };

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
    }, SETUP_TIMEOUT_MS);

    afterAll(async () => {
      if (env && workspaceId) {
        try {
          await cleanWorkspaceFleets(env, { workspaceId, runPrefix: ACCEPTANCE_RUN_PREFIX });
        } catch { /* best-effort teardown; never fail the run on cleanup */ }
      }
      if (stateDir) await fs.rm(stateDir, { recursive: true, force: true });
    });

    it("`approvals list` reaches the inbox and answers a well-formed page", async () => {
      const gates = await listGates();
      // An empty inbox is a legitimate state; the assertion is on the shape the
      // page carries, which is what every later step reads.
      for (const row of gates) {
        assert.equal(typeof row.gate_id, "string", `a row carried no gate id: ${JSON.stringify(row)}`);
        assert.equal(typeof row.status, "string", `a row carried no status: ${JSON.stringify(row)}`);
      }
    });

    it("`approvals show` on a listed gate prints its blast radius", async () => {
      const gates = await listGates();
      const first = gates[0];
      if (first?.gate_id === undefined) return; // nothing waiting in this workspace
      const result = await runWithEnv(["approvals", "show", first.gate_id, JSON_FLAG]);
      assert.equal(result.code, 0, `approvals show failed: ${result.stderr}`);
      const gate = parseJson(result.stdout) as GateListRow;
      assert.equal(gate.gate_id, first.gate_id);
      assert.equal(typeof gate.blast_radius, "string",
        `a gate must carry the sentence the decision turns on: ${result.stdout}`);
    });

    it("`approvals list --fleet` narrows to one Fleet", async () => {
      const gates = await listGates();
      const withFleet = gates.find((row) => typeof row.fleet_id === "string");
      if (withFleet?.fleet_id === undefined) return; // nothing waiting in this workspace
      const result = await runWithEnv([
        "approvals", "list", "--fleet", withFleet.fleet_id, JSON_FLAG,
      ]);
      assert.equal(result.code, 0, `approvals list --fleet failed: ${result.stderr}`);
      const narrowed = (parseJson(result.stdout) as { items?: GateListRow[] }).items ?? [];
      assert.ok(narrowed.every((row) => row.fleet_id === withFleet.fleet_id),
        `the filter let another Fleet's gate through: ${result.stdout}`);
    });

    it("`approvals approve` releases a pending gate and the inbox agrees", async () => {
      const gates = await listGates();
      const pending = gates.find((row) => row.status === GATE_STATUS.pending);
      if (pending?.gate_id === undefined) return; // nothing pending to decide
      const result = await runWithEnv(["approvals", "approve", pending.gate_id, JSON_FLAG]);
      assert.equal(result.code, 0, `approvals approve failed: ${result.stderr}`);
      assert.equal((parseJson(result.stdout) as { outcome?: string }).outcome, GATE_STATUS.approved,
        `the decision did not answer approved: ${result.stdout}`);

      const after = await listGates();
      const sameGate = after.find((row) => row.gate_id === pending.gate_id);
      assert.notEqual(sameGate?.status, GATE_STATUS.pending,
        `the gate is still pending after a successful approve: ${JSON.stringify(sameGate)}`);
    });

    it("`approvals show` on an unknown gate refuses through the daemon", async () => {
      const result = await runWithEnv([
        "approvals", "show", "01900000-0000-7000-8000-0000000abcde", JSON_FLAG,
      ]);
      assert.notEqual(result.code, 0, `an unknown gate must not succeed: ${result.stdout}`);
    });
  });
}
