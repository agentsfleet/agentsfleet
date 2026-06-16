/**
 * AGENTSFLEET_TOKEN-injection acceptance scenario.
 *
 * Mints a Clerk session JWT via the admin path (mirrors the dashboard
 * suite's identity), hydrates workspaces.json directly from the API
 * (the CLI only hydrates inside the login flow — the after-login spec
 * covers that path), then walks the full CLI surface:
 *   - install → status → logs → billing → stop → resume → kill
 *   - read-only sweep over READ_ONLY_COMMANDS
 *   - prefix-scoped post-teardown empty-list assertion
 *   - invalid-arg-value matrix with valid-format nonexistent IDs
 *   - invalid-format identifier rejected client-side, no network
 *   - missing-required-arg sweep over REQUIRES_POSITIONAL_ARG
 *
 * WS-E #C1 regression fires after every `runAgentctl` call: the minted
 * JWT must not appear in stdout/stderr.
 *
 * Live-only: the entire suite registers only when
 * `AGENTSFLEET_ACCEPTANCE_TARGET` is an https URL. Without that gate, all
 * tests are skipped — matches the unit-test runner's local invariant.
 */

import { describe, it, beforeAll, afterAll } from "bun:test";
import assert from "node:assert/strict";
import crypto from "node:crypto";
import fs from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import url from "node:url";

import {
  COMMAND_GROUPS,
  INVALID_ID_SAMPLES,
  PER_AGENTSFLEET_READ_ONLY_COMMANDS,
  READ_ONLY_COMMANDS,
  REQUIRES_IDENTIFIER,
  REQUIRES_POSITIONAL_ARG,
} from "./fixtures/command-matrix.ts";
import { ACCEPTANCE_RUN_PREFIX, TERMINAL_STATUSES, UNROUTABLE_API_URL } from "./fixtures/constants.ts";
import { composeEnv, runAgentctl } from "./fixtures/cli.js";
import type { RunResult } from "./fixtures/cli.js";
import {
  expectInvalidArgValue,
  expectMissingArg,
  assertNoConnectionError,
  assertNoSecretLeak,
} from "./fixtures/negatives.ts";
import {
  resolveAcceptanceEnv,
  resolveClerkSecret,
  resolveFixtureEmail,
} from "./global-setup.ts";
import { attachJwt } from "./fixtures/clerk-admin.ts";
import { hydrateWorkspacesForToken } from "./fixtures/workspace-hydration.ts";
import { installPlatformOpsAgent } from "./fixtures/seed.ts";
import { cleanWorkspaceAgents } from "./fixtures/teardown.ts";
import {
  killAgent,
  resumeAgent,
  stopAgent,
  expectStatus,
} from "./fixtures/lifecycle.ts";

const HERE = path.dirname(url.fileURLToPath(import.meta.url));
const CLI_ROOT = path.resolve(HERE, "..", "..");

const target = process.env.AGENTSFLEET_ACCEPTANCE_TARGET ?? "";
const isLive = target.startsWith("https://");

interface ValidateResult {
  readonly ok: boolean;
  readonly message: string;
}

interface ValidateModule {
  validateRequiredId(value: string, label: string): ValidateResult;
}

// Random uuidv7 for the invalid-arg-value sweep — backend's `isUuidV7`
// rejects v4, so `crypto.randomUUID()` would surface as a 400/validation
// error instead of 404. Hand-roll a v7 with valid version+variant bits
// and random payload so the server's not-found branch fires.
function randomUuidv7(): string {
  const bytes = crypto.randomBytes(16);
  const tsMs = BigInt(Date.now());
  bytes[0] = Number((tsMs >> 40n) & 0xffn);
  bytes[1] = Number((tsMs >> 32n) & 0xffn);
  bytes[2] = Number((tsMs >> 24n) & 0xffn);
  bytes[3] = Number((tsMs >> 16n) & 0xffn);
  bytes[4] = Number((tsMs >> 8n) & 0xffn);
  bytes[5] = Number(tsMs & 0xffn);
  bytes[6] = ((bytes[6] ?? 0) & 0x0f) | 0x70;
  bytes[8] = ((bytes[8] ?? 0) & 0x3f) | 0x80;
  const hex = bytes.toString("hex");
  return `${hex.slice(0, 8)}-${hex.slice(8, 12)}-${hex.slice(12, 16)}-${hex.slice(16, 20)}-${hex.slice(20)}`;
}

let validateModule: ValidateModule;

if (!isLive) {
  describe("lifecycle-with-token.spec.ts", () => {
    it.skip("requires AGENTSFLEET_ACCEPTANCE_TARGET to be an https URL", () => {});
  });
} else {
  describe("lifecycle-with-token — AGENTSFLEET_TOKEN injection", () => {
    let apiUrl: string = "";
    let sessionJwt: string = "";
    let stateDir: string = "";
    let env: Record<string, string> = {};
    let workspaceId: string = "";

    async function runWithEnv(
      args: ReadonlyArray<string>,
      extraEnv?: Record<string, string>,
    ): Promise<RunResult> {
      const composed = extraEnv ? { ...env, ...extraEnv } : env;
      const result = await runAgentctl(args, { env: composed });
      assertNoSecretLeak(result, sessionJwt);
      return result;
    }

    beforeAll(async () => {
      apiUrl = resolveAcceptanceEnv().apiUrl;
      const clerkSecret = resolveClerkSecret();
      const email = resolveFixtureEmail("regular");
      const minted = await attachJwt(clerkSecret, { email });
      sessionJwt = minted.sessionJwt;

      stateDir = await fs.mkdtemp(path.join(os.tmpdir(), "agentsfleet-token-"));
      env = composeEnv({
        AGENTSFLEET_TOKEN: sessionJwt,
        AGENTSFLEET_API_URL: apiUrl,
        AGENTSFLEET_STATE_DIR: stateDir,
        NO_COLOR: "1",
      });
      const hydrated = await hydrateWorkspacesForToken({ apiUrl, token: sessionJwt, stateDir });
      workspaceId = hydrated.currentWorkspaceId;

      validateModule = await import(path.join(CLI_ROOT, "src/program/validators.ts")) as ValidateModule;
    });

    afterAll(async () => {
      if (env && workspaceId) {
        try { await cleanWorkspaceAgents(env, { workspaceId }); } catch { /* best-effort teardown */ }
      }
      if (stateDir) await fs.rm(stateDir, { recursive: true, force: true });
    });

    // Full lifecycle walk against a freshly-installed agent.
    describe("lifecycle walk", () => {
      let agentId: string = "";

      it("install platform-ops bundle", async () => {
        const installed = await installPlatformOpsAgent({ env });
        assert.ok(installed.id || installed.agent_id, `install response missing id: ${JSON.stringify(installed)}`);
        const id = installed.id ?? installed.agent_id;
        if (!id) throw new Error("install missing id");
        agentId = id;
      });

      // Per-agent read-only sweep — runs against the live agentId so
      // commands like `grant list` (which require `--agent <id>`) get
      // exercised inside the lifecycle suite instead of forcing
      // fixture state into the workspace-wide READ_ONLY_COMMANDS table.
      for (const row of PER_AGENTSFLEET_READ_ONLY_COMMANDS) {
        const label = `${row.argsHead.join(" ")} --agent <id>`;
        it(`${label} exits 0 with parseable JSON`, async () => {
          const args = [...row.argsHead, "--agent", agentId, "--json"];
          const result = await runWithEnv(args);
          assert.equal(result.code, 0, `${label} exited ${result.code}: ${result.stderr}`);
          const parsed = JSON.parse(result.stdout.trim()) as Record<string, unknown>;
          if (row.requiredKey) {
            assert.ok(row.requiredKey in parsed, `${label}: missing ${row.requiredKey} in ${result.stdout}`);
          }
          if (row.isList && row.itemsKey) {
            assert.ok(Array.isArray(parsed[row.itemsKey]), `${label}: ${row.itemsKey} not an array`);
          }
        });
      }

      it("status reports active", async () => {
        const payload = await expectStatus(env, agentId, ["active", "starting", "running"]);
        assert.equal(typeof payload.status, "string");
      });

      it("logs --json returns a parseable envelope", async () => {
        // `--since` lives on `events`, NOT `logs` (`logs` only takes
        // `--agent`, `--limit`, `--cursor`); commander would exit 1 on
        // an unknown flag. The recency bound here was misplaced — the
        // intent is just to exercise the read path on a real agent.
        const result = await runWithEnv(["logs", "--agent", agentId, "--json"]);
        assert.equal(result.code, 0, `logs exited ${result.code}: ${result.stderr}`);
        const parsed = JSON.parse(result.stdout.trim() || "{}");
        assert.equal(typeof parsed, "object");
      });

      it("billing show --json returns a balance field", async () => {
        const result = await runWithEnv(["billing", "show", "--json"]);
        assert.equal(result.code, 0, `billing show exited ${result.code}: ${result.stderr}`);
        const parsed = JSON.parse(result.stdout.trim());
        assert.ok("balance_nanos" in parsed, `billing response missing balance_nanos: ${result.stdout}`);
      });

      it("stop → resume → kill walks state", async () => {
        await stopAgent(env, agentId);
        await expectStatus(env, agentId, ["paused", "stopped"]);
        await resumeAgent(env, agentId);
        await expectStatus(env, agentId, ["active", "running", "starting"]);
        await killAgent(env, agentId);
        await expectStatus(env, agentId, ["killed", "errored", "terminated"]);
      }, 30_000);

      it("kill is idempotent on a terminal agent", async () => {
        const result = await runWithEnv(["kill", agentId, "--json"]);
        // Either succeed silently, surface an already-terminal stem, or report
        // not-found after the terminal transition hides the agent from writes.
        // What's not acceptable is re-emitting `status: active` later. The
        // status assertion below catches that.
        if (result.code !== 0) {
          assert.match(result.stderr + result.stdout, /UZ-AGT-010|already.*terminal|killed|terminated|HTTP_404|not found/i);
        }
        await expectStatus(env, agentId, ["killed", "errored", "terminated"]);
      });
    });

    // Workspace-wide read-only sweep.
    describe("read-only sweep", () => {
      for (const row of READ_ONLY_COMMANDS) {
        const label = row.label ?? row.args.join(" ");
        it(`${label} exits 0 with parseable JSON`, async () => {
          const result = await runWithEnv(row.args);
          assert.equal(result.code, 0, `${label} exited ${result.code}: ${result.stderr}`);
          const parsed = JSON.parse(result.stdout.trim()) as Record<string, unknown>;
          if (row.requiredKey) {
            assert.ok(row.requiredKey in parsed, `${label}: missing ${row.requiredKey} in ${result.stdout}`);
          }
          if (row.isList && row.itemsKey) {
            assert.ok(Array.isArray(parsed[row.itemsKey]), `${label}: ${row.itemsKey} not an array`);
          }
        });
      }
    });

    // Prefix-scoped post-teardown emptiness — shared DEV tenants carry
    // residual agents, so the contract is "none of MY run's agents
    // remain alive", not "the workspace is globally empty". Terminal
    // (killed/errored/terminated) rows still surface in the list and
    // prove teardown worked — filter them out before asserting.
    describe("post-teardown emptiness (prefix-scoped)", () => {
      beforeAll(async () => {
        await cleanWorkspaceAgents(env, { workspaceId, runPrefix: ACCEPTANCE_RUN_PREFIX });
      });

      it(`agent list --json: no LIVE items match ACCEPTANCE_RUN_PREFIX`, async () => {
        const result = await runWithEnv(["list", "--json"]);
        assert.equal(result.code, 0, `list --json exited ${result.code}: ${result.stderr}`);
        const parsed = JSON.parse(result.stdout.trim()) as { items?: unknown };
        const items = Array.isArray(parsed.items) ? (parsed.items as Array<{ name?: string; status?: string }>) : [];
        const mineLive = items.filter((z) =>
          typeof z.name === "string" &&
          z.name.startsWith(ACCEPTANCE_RUN_PREFIX) &&
          !TERMINAL_STATUSES.includes(z.status ?? ""),
        );
        assert.equal(mineLive.length, 0,
          `expected zero live agents starting with ${ACCEPTANCE_RUN_PREFIX}; got ${mineLive.length}: ${JSON.stringify(mineLive)}`);
      });
    });

    // Valid-format nonexistent UUID → server 404 → UZ-* envelope.
    describe("invalid-arg-value (valid format, nonexistent)", () => {
      for (const row of REQUIRES_IDENTIFIER) {
        if (!row.apiHits) continue;
        it(`${row.args.join(" ")} <random-uuidv7> → ${row.expectedErrorCode}`, async () => {
          const id = randomUuidv7();
          const result = await expectInvalidArgValue([...row.args, id, "--json"], env, row.expectedErrorCode);
          assertNoSecretLeak(result, sessionJwt);
        });
      }
    });

    // Invalid-format ID rejected client-side; no network call fires.
    // Today only workspace use/delete run `validateRequiredId`. The agent /
    // agent / grant handlers send invalid strings straight to the API —
    // surfaced as Discovery (CLI hygiene: wire validateRequiredId into the
    // remaining ID-taking handlers, then this sweep widens automatically).
    describe("invalid-format ID — client-side rejection, no network", () => {
      // All INVALID_ID_SAMPLES fail the uuidv7 validator introduced in this
      // PR (SAFE_ID_RE was removed). Run the full set so every sample is
      // confirmed to be rejected client-side without touching the network.
      const rejectingSamples = INVALID_ID_SAMPLES;
      assert.ok(rejectingSamples.length >= 1, "INVALID_ID_SAMPLES must include at least one stem that fails SAFE_ID_RE");

      for (const row of REQUIRES_IDENTIFIER) {
        if (!row.validatesClient) continue;
        for (const sample of rejectingSamples) {
          it(`${row.args.join(" ")} "${sample}" rejected without ECONNREFUSED`, async () => {
            const unroutable = composeEnv({
              AGENTSFLEET_TOKEN: sessionJwt,
              AGENTSFLEET_API_URL: UNROUTABLE_API_URL,
              AGENTSFLEET_STATE_DIR: stateDir,
              NO_COLOR: "1",
            });
            const result = await runAgentctl([...row.args, sample, "--json"], { env: unroutable });
            assert.notEqual(result.code, 0, `expected non-zero exit; stdout=${result.stdout} stderr=${result.stderr}`);
            assertNoConnectionError(result, [...row.args, sample]);
            assertNoSecretLeak(result, sessionJwt);
            const liveStem = validateModule.validateRequiredId(sample, row.argName).message;
            assert.ok(result.stdout.includes(liveStem) || result.stderr.includes(liveStem),
              `expected validator stem "${liveStem}"; got stdout=${result.stdout} stderr=${result.stderr}`);
          });
        }
      }
    });

    // Missing-required-arg sweep — lives in the lifecycle suite because
    // the CLI checks workspace-context before arg-validation, so this
    // path only fires when fixture state exists.
    describe("missing-required positional arg", () => {
      for (const row of REQUIRES_POSITIONAL_ARG) {
        it(`${row.args.join(" ")} (no <${row.missingArgName}>) exits non-zero`, async () => {
          const result = await expectMissingArg(row.args, env);
          assertNoSecretLeak(result, sessionJwt);
        });
      }
    });

    // Coverage check — every COMMAND_GROUP exercised somewhere in this suite
    // (workspace-wide read-only sweep + per-agent sweep together cover
    // workspace/agent/grant/tenant/billing/agent).
    it("touch every COMMAND_GROUP via the read-only sweep", () => {
      const exercised = new Set<string>();
      for (const row of READ_ONLY_COMMANDS) {
        const head = row.args[0];
        if (!head) continue;
        if (head === "list" || head === "doctor") exercised.add("agent");
        if (COMMAND_GROUPS.includes(head)) exercised.add(head);
      }
      for (const row of PER_AGENTSFLEET_READ_ONLY_COMMANDS) {
        if (row.group) exercised.add(row.group);
      }
      const missing = COMMAND_GROUPS.filter((g) => !exercised.has(g) && g !== "agent");
      assert.deepEqual(missing, [], `command groups missing from read-only sweep: ${missing.join(",")}`);
    });
  });
}
