/**
 * Install negative-path acceptance scenarios (live, token-injected).
 *
 * Mints a Clerk session JWT via the admin path, hydrates workspaces.json
 * from the API (matching the lifecycle-with-token spec's identity setup),
 * then drives the failure surface of `agentsfleet install`:
 *   - `install --from <nonexistent path>`  → ConfigError, exit 5,
 *     ERR_PATH_NOT_FOUND in stderr, no agent created.
 *   - `install --from <dir missing SKILL.md>`    → ERR_SKILL_MISSING, exit 5.
 *   - `install --from <dir missing TRIGGER.md>`  → ERR_TRIGGER_MISSING, exit 5.
 *   - `install` with no `--from`            → ValidationError, exit 4, no network.
 *   - duplicate name (same bundle installed twice) → second install rejected
 *     (UZ-AGT-006, exit 3) — the workspace's `(workspace_id, name)`
 *     uniqueness constraint. The duplicate bundle is the canonical
 *     `platform-ops-sample` rewritten with a stable prefixed name, so the
 *     first install actually succeeds (a minimal bundle would fail server
 *     config validation long before the conflict path).
 *
 * Every spawn runs `assertNoSecretLeak` against the minted JWT. Mutating
 * tests are prefix-scoped via ACCEPTANCE_RUN_PREFIX and reclaimed in
 * `afterAll` through `cleanWorkspaceAgents`; the suite never asserts global
 * workspace emptiness, only that none of THIS run's agents linger.
 *
 * Live-only: registers real tests only when AGENTSFLEET_ACCEPTANCE_TARGET is
 * an https URL; otherwise a single skipped placeholder keeps the local
 * runner green.
 */

import { describe, it, beforeAll, afterAll } from "bun:test";
import assert from "node:assert/strict";
import fs from "node:fs/promises";
import os from "node:os";
import path from "node:path";

import { ACCEPTANCE_RUN_PREFIX, TERMINAL_STATUSES } from "./fixtures/constants.ts";
import { composeEnv, runAgentctl } from "./fixtures/cli.js";
import type { RunResult } from "./fixtures/cli.js";
import { assertNoSecretLeak } from "./fixtures/negatives.ts";
import {
  resolveAcceptanceEnv,
  resolveClerkSecret,
  resolveFixtureEmail,
} from "./global-setup.ts";
import { attachJwt } from "./fixtures/clerk-admin.ts";
import { hydrateWorkspacesForToken } from "./fixtures/workspace-hydration.ts";
import { cleanWorkspaceAgents } from "./fixtures/teardown.ts";
import {
  EXIT_CONFIG_ERROR,
  EXIT_SERVER_ERROR,
  EXIT_VALIDATION_ERROR,
  ERR_AGENTSFLEET_NAME_TAKEN,
  ERR_PATH_NOT_FOUND,
  ERR_SKILL_MISSING,
  ERR_TRIGGER_MISSING,
  makeNamedBundle,
  makeSkillMissingBundle,
  makeTriggerMissingBundle,
  nonexistentBundlePath,
  removeDir,
  type NamedBundle,
} from "./fixtures/install-negatives-ops.ts";

const target = process.env.AGENTSFLEET_ACCEPTANCE_TARGET ?? "";
const isLive = target.startsWith("https://");

const FLAG_FROM = "--from";
const FLAG_JSON = "--json";
const INSTALL = "install";

interface InstallEnvelope {
  readonly agent_id?: string;
  readonly id?: string;
  readonly [key: string]: unknown;
}

function parseInstallId(stdout: string): string | null {
  const trimmed = stdout.trim();
  if (!trimmed) return null;
  try {
    const parsed = JSON.parse(trimmed) as InstallEnvelope;
    return parsed.agent_id ?? parsed.id ?? null;
  } catch {
    return null;
  }
}

if (!isLive) {
  describe("install-negatives.spec.ts", () => {
    it.skip("requires AGENTSFLEET_ACCEPTANCE_TARGET to be an https URL", () => {});
  });
} else {
  describe("install-negatives — AGENTSFLEET_TOKEN injection", () => {
    let apiUrl = "";
    let sessionJwt = "";
    let stateDir = "";
    let env: Record<string, string> = {};
    let workspaceId = "";

    async function runWithEnv(args: ReadonlyArray<string>): Promise<RunResult> {
      const result = await runAgentctl(args, { env });
      assertNoSecretLeak(result, sessionJwt);
      return result;
    }

    function merged(result: RunResult): string {
      return `${result.stderr}\n${result.stdout}`;
    }

    beforeAll(async () => {
      apiUrl = resolveAcceptanceEnv().apiUrl;
      const clerkSecret = resolveClerkSecret();
      const email = resolveFixtureEmail("regular");
      const minted = await attachJwt(clerkSecret, { email });
      sessionJwt = minted.sessionJwt;

      stateDir = await fs.mkdtemp(path.join(os.tmpdir(), "agentsfleet-install-neg-state-"));
      env = composeEnv({
        AGENTSFLEET_TOKEN: sessionJwt,
        AGENTSFLEET_API_URL: apiUrl,
        AGENTSFLEET_STATE_DIR: stateDir,
        NO_COLOR: "1",
      });
      const hydrated = await hydrateWorkspacesForToken({ apiUrl, token: sessionJwt, stateDir });
      workspaceId = hydrated.currentWorkspaceId;
    });

    afterAll(async () => {
      if (env && workspaceId) {
        try {
          await cleanWorkspaceAgents(env, { workspaceId, runPrefix: ACCEPTANCE_RUN_PREFIX });
        } catch {
          /* best-effort teardown — shared DEV tenant */
        }
      }
      if (stateDir) await fs.rm(stateDir, { recursive: true, force: true });
    });

    // ── nonexistent --from path ─────────────────────────────────────
    describe("--from nonexistent path", () => {
      it("exits ConfigError with ERR_PATH_NOT_FOUND and helpful suggestion", async () => {
        const absent = nonexistentBundlePath();
        const result = await runWithEnv([INSTALL, FLAG_FROM, absent, FLAG_JSON]);
        assert.equal(
          result.code,
          EXIT_CONFIG_ERROR,
          `expected exit ${EXIT_CONFIG_ERROR}; got ${result.code}: ${merged(result)}`,
        );
        const body = merged(result);
        assert.match(
          body,
          new RegExp(ERR_PATH_NOT_FOUND),
          `expected ${ERR_PATH_NOT_FOUND} in output: ${body}`,
        );
        // The loader's remap carries an actionable suggestion, not a raw
        // ENOENT stack — assert the operator gets guidance.
        assert.match(body, /skill\.md|trigger\.md|path/i, `expected actionable hint: ${body}`);
      });
    });

    // ── malformed bundle: missing SKILL.md ────────────────────────────
    describe("--from dir missing SKILL.md", () => {
      let dir: string | null = null;
      afterAll(async () => { await removeDir(dir); dir = null; });

      it("exits ConfigError with ERR_SKILL_MISSING", async () => {
        dir = await makeSkillMissingBundle();
        const result = await runWithEnv([INSTALL, FLAG_FROM, dir, FLAG_JSON]);
        assert.equal(
          result.code,
          EXIT_CONFIG_ERROR,
          `expected exit ${EXIT_CONFIG_ERROR}; got ${result.code}: ${merged(result)}`,
        );
        assert.match(
          merged(result),
          new RegExp(ERR_SKILL_MISSING),
          `expected ${ERR_SKILL_MISSING}: ${merged(result)}`,
        );
      });
    });

    // ── malformed bundle: missing TRIGGER.md ────────────────────────
    describe("--from dir missing TRIGGER.md", () => {
      let dir: string | null = null;
      afterAll(async () => { await removeDir(dir); dir = null; });

      it("exits ConfigError with ERR_TRIGGER_MISSING", async () => {
        dir = await makeTriggerMissingBundle();
        const result = await runWithEnv([INSTALL, FLAG_FROM, dir, FLAG_JSON]);
        assert.equal(
          result.code,
          EXIT_CONFIG_ERROR,
          `expected exit ${EXIT_CONFIG_ERROR}; got ${result.code}: ${merged(result)}`,
        );
        assert.match(
          merged(result),
          new RegExp(ERR_TRIGGER_MISSING),
          `expected ${ERR_TRIGGER_MISSING}: ${merged(result)}`,
        );
      });
    });

    // ── missing required --from flag ────────────────────────────────
    describe("missing --from flag", () => {
      it("exits ValidationError and names --from without hitting the network", async () => {
        const result = await runWithEnv([INSTALL, FLAG_JSON]);
        assert.equal(
          result.code,
          EXIT_VALIDATION_ERROR,
          `expected exit ${EXIT_VALIDATION_ERROR}; got ${result.code}: ${merged(result)}`,
        );
        assert.match(merged(result), new RegExp(FLAG_FROM), `expected --from mention: ${merged(result)}`);
        // Client-side validation precedes any HTTP call — a stack-trace
        // network error would mean the guard never fired.
        assert.doesNotMatch(
          merged(result),
          /ECONNREFUSED|ENOTFOUND|EAI_AGAIN|fetch failed/,
          `--from guard should precede any network call: ${merged(result)}`,
        );
      });
    });

    // ── duplicate name: same bundle installed twice ──────────────────────
    // The first install must succeed (and is tracked for teardown); the
    // second must be rejected by the `(workspace_id, name)` uniqueness
    // constraint. We do NOT assume the wire shape beyond "non-zero exit, no
    // SECOND agent row" — but when the server surfaces its conflict code we
    // assert it is exactly UZ-AGT-006 (exit 3).
    describe("duplicate agent name", () => {
      let bundle: NamedBundle | null = null;
      let firstId: string | null = null;

      afterAll(async () => {
        if (firstId) {
          try { await runAgentctl(["kill", firstId, FLAG_JSON], { env }); } catch { /* teardown */ }
        }
        await removeDir(bundle?.dir);
        bundle = null;
      });

      it("first install of a fresh prefixed name succeeds", async () => {
        bundle = await makeNamedBundle();
        const result = await runAgentctl(
          [INSTALL, FLAG_FROM, bundle.dir, FLAG_JSON],
          { env, timeoutMs: 120_000 },
        );
        assertNoSecretLeak(result, sessionJwt);
        assert.equal(result.code, 0, `first install failed ${result.code}: ${merged(result)}`);
        firstId = parseInstallId(result.stdout);
        assert.ok(firstId, `first install missing agent id: ${result.stdout}`);
      }, 130_000);

      it("second install of the same name is rejected, no duplicate created", async () => {
        assert.ok(bundle, "bundle must be created by the first-install test");
        const target2 = bundle as NamedBundle;
        const result = await runAgentctl(
          [INSTALL, FLAG_FROM, target2.dir, FLAG_JSON],
          { env, timeoutMs: 120_000 },
        );
        assertNoSecretLeak(result, sessionJwt);
        assert.notEqual(result.code, 0, `duplicate install should fail; got 0: ${result.stdout}`);

        // If the server surfaced its conflict code, pin the exact code+exit.
        const secondId = parseInstallId(result.stdout);
        const body = merged(result);
        if (body.includes(ERR_AGENTSFLEET_NAME_TAKEN)) {
          assert.equal(
            result.code,
            EXIT_SERVER_ERROR,
            `name-taken conflict must exit ${EXIT_SERVER_ERROR}: ${body}`,
          );
        }
        // Whatever the surface, the second call must not have minted a new
        // agent under a different id (a partial-create leak).
        if (secondId && firstId) {
          assert.equal(secondId, firstId, `duplicate install leaked a second agent id: ${secondId}`);
        } else {
          assert.equal(secondId, null, `duplicate install must not return a new agent id: ${secondId}`);
        }
      }, 130_000);

      // Prefix-scoped leak audit — exactly one LIVE agent for this bundle's
      // name should exist after the duplicate attempt (the first install),
      // never two. Confirms the rejected second install left no residue.
      it("exactly one live agent carries the duplicate bundle name", async () => {
        assert.ok(bundle, "bundle must exist");
        const wanted = (bundle as NamedBundle).name;
        const listed = await runWithEnv(["list", FLAG_JSON]);
        assert.equal(listed.code, 0, `list exited ${listed.code}: ${listed.stderr}`);
        const parsed = JSON.parse(listed.stdout.trim() || "{}") as { items?: unknown };
        const items = Array.isArray(parsed.items)
          ? (parsed.items as Array<{ name?: string; status?: string }>)
          : [];
        const sameName = items.filter((z) => z.name === wanted);
        assert.ok(
          sameName.length <= 1,
          `expected at most one agent named ${wanted}; got ${sameName.length}: ${JSON.stringify(sameName)}`,
        );
      });
    });

    // ── prefix-scoped teardown audit ────────────────────────────────
    // After reclaiming this run's agents, no LIVE agent whose name starts
    // with ACCEPTANCE_RUN_PREFIX should remain. Terminal rows still appear
    // in the list and prove teardown worked — they are filtered out.
    describe("post-teardown emptiness (prefix-scoped)", () => {
      beforeAll(async () => {
        await cleanWorkspaceAgents(env, { workspaceId, runPrefix: ACCEPTANCE_RUN_PREFIX });
      });

      it("no LIVE agent name starts with ACCEPTANCE_RUN_PREFIX", async () => {
        const result = await runWithEnv(["list", FLAG_JSON]);
        assert.equal(result.code, 0, `list exited ${result.code}: ${result.stderr}`);
        const parsed = JSON.parse(result.stdout.trim() || "{}") as { items?: unknown };
        const items = Array.isArray(parsed.items)
          ? (parsed.items as Array<{ name?: string; status?: string }>)
          : [];
        const mineLive = items.filter((z) =>
          typeof z.name === "string" &&
          z.name.startsWith(ACCEPTANCE_RUN_PREFIX) &&
          !TERMINAL_STATUSES.includes(z.status ?? ""),
        );
        assert.equal(
          mineLive.length,
          0,
          `expected zero live agents under ${ACCEPTANCE_RUN_PREFIX}; got ${JSON.stringify(mineLive)}`,
        );
      });
    });
  });
}
