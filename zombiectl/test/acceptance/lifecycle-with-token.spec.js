/**
 * ZOMBIE_TOKEN-injection acceptance scenario.
 *
 * Mints a Clerk session JWT via the admin path (mirrors the dashboard
 * suite's identity), hydrates workspaces.json directly from the API
 * (the CLI only hydrates inside commandLogin — §5 covers that path),
 * then walks the full CLI surface:
 *   §4a — install → status → logs → billing → stop → resume → kill
 *   §4b — read-only sweep over READ_ONLY_COMMANDS
 *   §4b' — empty-list standard message (post-teardown)
 *   §4c1 — invalid-arg-value matrix with valid-format nonexistent ID
 *   §4c2 — invalid-format identifier rejected client-side, no network
 *   §3-residual — missing-required-arg sweep over REQUIRES_POSITIONAL_ARG
 *
 * WS-E #C1 regression fires after every `runZombiectl` call: the minted
 * JWT must not appear in stdout/stderr. WS-E #C3 is §5's territory.
 *
 * Live-only: the entire suite registers only when
 * `ZOMBIE_ACCEPTANCE_TARGET` is an https URL. Without that gate, all
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
  EMPTY_LIST_CONVENTIONS,
  INVALID_ID_SAMPLES,
  PER_ZOMBIE_READ_ONLY_COMMANDS,
  READ_ONLY_COMMANDS,
  REQUIRES_IDENTIFIER,
  REQUIRES_POSITIONAL_ARG,
} from "./fixtures/command-matrix.js";
import { UNROUTABLE_API_URL } from "./fixtures/constants.js";
import { composeEnv, runZombiectl } from "./fixtures/cli.js";
import {
  expectInvalidArgValue,
  expectMissingArg,
  assertNoConnectionError,
  assertNoSecretLeak,
} from "./fixtures/negatives.js";
import {
  resolveAcceptanceEnv,
  resolveClerkSecret,
  resolveFixtureEmail,
} from "./global-setup.js";
import { attachJwt } from "./fixtures/clerk-admin.js";
import { hydrateWorkspacesForToken } from "./fixtures/workspace-hydration.js";
import { installPlatformOpsZombie } from "./fixtures/seed.js";
import { cleanWorkspaceZombies } from "./fixtures/teardown.js";
import {
  killZombie,
  resumeZombie,
  stopZombie,
  expectStatus,
} from "./fixtures/lifecycle.js";

const HERE = path.dirname(url.fileURLToPath(import.meta.url));
const ZOMBIECTL_ROOT = path.resolve(HERE, "..", "..");

const target = process.env.ZOMBIE_ACCEPTANCE_TARGET ?? "";
const isLive = target.startsWith("https://");

// Random uuidv7 for §4c1 — backend's `isUuidV7` rejects v4, so
// crypto.randomUUID() would surface as a 400/validation error instead
// of 404. Hand-roll a v7 with valid version+variant bits and random
// payload so the server's not-found branch fires.
function randomUuidv7() {
  const bytes = crypto.randomBytes(16);
  const tsMs = BigInt(Date.now());
  bytes[0] = Number((tsMs >> 40n) & 0xffn);
  bytes[1] = Number((tsMs >> 32n) & 0xffn);
  bytes[2] = Number((tsMs >> 24n) & 0xffn);
  bytes[3] = Number((tsMs >> 16n) & 0xffn);
  bytes[4] = Number((tsMs >> 8n) & 0xffn);
  bytes[5] = Number(tsMs & 0xffn);
  bytes[6] = (bytes[6] & 0x0f) | 0x70;
  bytes[8] = (bytes[8] & 0x3f) | 0x80;
  const hex = bytes.toString("hex");
  return `${hex.slice(0, 8)}-${hex.slice(8, 12)}-${hex.slice(12, 16)}-${hex.slice(16, 20)}-${hex.slice(20)}`;
}

let validateModule;

if (!isLive) {
  describe("lifecycle-with-token.spec.js", () => {
    it.skip("requires ZOMBIE_ACCEPTANCE_TARGET to be an https URL", () => {});
  });
} else {
  describe("lifecycle-with-token — ZOMBIE_TOKEN injection", () => {
    let apiUrl;
    let sessionJwt;
    let stateDir;
    let env;
    let workspaceId;

    async function spawn(args, extraEnv) {
      const composed = extraEnv ? { ...env, ...extraEnv } : env;
      const result = await runZombiectl(args, { env: composed });
      assertNoSecretLeak(result, sessionJwt);
      return result;
    }

    beforeAll(async () => {
      apiUrl = resolveAcceptanceEnv().apiUrl;
      const clerkSecret = resolveClerkSecret();
      const email = resolveFixtureEmail("regular");
      const minted = await attachJwt(clerkSecret, { email });
      sessionJwt = minted.sessionJwt;

      stateDir = await fs.mkdtemp(path.join(os.tmpdir(), "zombiectl-token-"));
      env = composeEnv({
        ZOMBIE_TOKEN: sessionJwt,
        ZOMBIE_API_URL: apiUrl,
        ZOMBIE_STATE_DIR: stateDir,
        NO_COLOR: "1",
      });
      const hydrated = await hydrateWorkspacesForToken({ apiUrl, token: sessionJwt, stateDir });
      workspaceId = hydrated.currentWorkspaceId;

      validateModule = await import(path.join(ZOMBIECTL_ROOT, "src/program/validate.ts"));
    });

    afterAll(async () => {
      if (env && workspaceId) {
        try { await cleanWorkspaceZombies(env, workspaceId); } catch {}
      }
      if (stateDir) await fs.rm(stateDir, { recursive: true, force: true });
    });

    // §4a — full lifecycle walk
    describe("§4a lifecycle walk", () => {
      let zombieId;

      it("install platform-ops bundle", async () => {
        const installed = await installPlatformOpsZombie({ env });
        assert.ok(installed.id || installed.zombie_id, `install response missing id: ${JSON.stringify(installed)}`);
        zombieId = installed.id ?? installed.zombie_id;
      });

      // Per-zombie read-only sweep — runs against the live zombieId so
      // commands like `grant list` (which require `--zombie <id>`) get
      // exercised inside the lifecycle suite instead of forcing
      // fixture state into the workspace-wide READ_ONLY_COMMANDS table.
      for (const row of PER_ZOMBIE_READ_ONLY_COMMANDS) {
        const label = `${row.argsHead.join(" ")} --zombie <id>`;
        it(`${label} exits 0 with parseable JSON`, async () => {
          const args = [...row.argsHead, "--zombie", zombieId, "--json"];
          const result = await spawn(args);
          assert.equal(result.code, 0, `${label} exited ${result.code}: ${result.stderr}`);
          const parsed = JSON.parse(result.stdout.trim());
          if (row.requiredKey) {
            assert.ok(row.requiredKey in parsed, `${label}: missing ${row.requiredKey} in ${result.stdout}`);
          }
          if (row.isList) {
            assert.ok(Array.isArray(parsed[row.itemsKey]), `${label}: ${row.itemsKey} not an array`);
          }
        });
      }

      it("status reports active", async () => {
        const payload = await expectStatus(env, zombieId, ["active", "starting", "running"]);
        assert.equal(typeof payload.status, "string");
      });

      it("logs --json returns a parseable envelope", async () => {
        // `--since` lives on `events`, NOT `logs` (`logs` only takes
        // `--zombie`, `--limit`, `--cursor`); commander would exit 1 on
        // an unknown flag. The recency bound here was misplaced — the
        // intent is just to exercise the read path on a real zombie.
        const result = await spawn(["logs", zombieId, "--json"]);
        assert.equal(result.code, 0, `logs exited ${result.code}: ${result.stderr}`);
        const parsed = JSON.parse(result.stdout.trim() || "{}");
        assert.equal(typeof parsed, "object");
      });

      it("billing show --json returns a balance field", async () => {
        const result = await spawn(["billing", "show", "--json"]);
        assert.equal(result.code, 0, `billing show exited ${result.code}: ${result.stderr}`);
        const parsed = JSON.parse(result.stdout.trim());
        assert.ok("balance" in parsed, `billing response missing balance: ${result.stdout}`);
      });

      it("stop → resume → kill walks state", async () => {
        await stopZombie(env, zombieId);
        await expectStatus(env, zombieId, ["paused", "stopped"]);
        await resumeZombie(env, zombieId);
        await expectStatus(env, zombieId, ["active", "running", "starting"]);
        await killZombie(env, zombieId);
        await expectStatus(env, zombieId, ["killed", "errored", "terminated"]);
      });

      it("kill is idempotent on a terminal zombie", async () => {
        const result = await spawn(["kill", zombieId, "--json"]);
        // Either succeed silently, or surface UZ-ZMB-010 (already terminal).
        // Both are acceptable — what's NOT acceptable is exiting 0 then re-emitting
        // a `status: active` later. The status assertion below catches that.
        if (result.code !== 0) {
          assert.match(result.stderr + result.stdout, /UZ-ZMB-010|already.*terminal|killed|terminated/i);
        }
        await expectStatus(env, zombieId, ["killed", "errored", "terminated"]);
      });
    });

    // §4b — read-only sweep
    describe("§4b read-only sweep", () => {
      for (const row of READ_ONLY_COMMANDS) {
        const label = row.label ?? row.args.join(" ");
        it(`${label} exits 0 with parseable JSON`, async () => {
          const result = await spawn(row.args);
          assert.equal(result.code, 0, `${label} exited ${result.code}: ${result.stderr}`);
          const parsed = JSON.parse(result.stdout.trim());
          if (row.requiredKey) {
            assert.ok(row.requiredKey in parsed, `${label}: missing ${row.requiredKey} in ${result.stdout}`);
          }
          if (row.isList) {
            assert.ok(Array.isArray(parsed[row.itemsKey]), `${label}: ${row.itemsKey} not an array`);
          }
        });
      }
    });

    // §4b' — empty-list standard message (post-teardown).
    //
    // Only `zombie list` is asserted empty: cleanWorkspaceZombies kills the
    // fixture-installed zombies, leaving the list provably empty. workspace /
    // agent / grant lists may carry residual state across CI runs (shared
    // tenant identity) — those rows surface as Discovery once a per-suite
    // tenant teardown lands.
    describe("§4b' empty-list message (zombie list)", () => {
      beforeAll(async () => {
        await cleanWorkspaceZombies(env, workspaceId);
      });

      const stem = EMPTY_LIST_CONVENTIONS["list"];

      it(`zombie list --json: items array is empty`, async () => {
        const result = await spawn(["list", "--json"]);
        assert.equal(result.code, 0, `list --json exited ${result.code}: ${result.stderr}`);
        const parsed = JSON.parse(result.stdout.trim());
        assert.ok(Array.isArray(parsed.items) && parsed.items.length === 0,
          `expected empty items array; got: ${result.stdout}`);
      });

      it(`zombie list (non-JSON) emits "${stem}"`, async () => {
        const result = await spawn(["list"]);
        assert.equal(result.code, 0, `list exited ${result.code}: ${result.stderr}`);
        assert.match(result.stdout.toLowerCase(), new RegExp(stem.toLowerCase()),
          `missing stem "${stem}" in: ${result.stdout}`);
      });
    });

    // §4c1 — valid-format nonexistent UUID → server 404 → UZ-* envelope
    describe("§4c1 invalid-arg-value (valid format, nonexistent)", () => {
      for (const row of REQUIRES_IDENTIFIER) {
        if (!row.apiHits) continue;
        it(`${row.args.join(" ")} <random-uuidv7> → ${row.expectedErrorCode}`, async () => {
          const id = randomUuidv7();
          const result = await expectInvalidArgValue([...row.args, id, "--json"], env, row.expectedErrorCode);
          assertNoSecretLeak(result, sessionJwt);
        });
      }
    });

    // §4c2 — invalid-format ID rejected client-side; no network call fires.
    // Today only workspace use/delete run `validateRequiredId`. The zombie /
    // agent / grant handlers send invalid strings straight to the API —
    // surfaced as Discovery (CLI hygiene: wire validateRequiredId into the
    // remaining ID-taking handlers, then this sweep widens automatically).
    describe("§4c2 invalid-format ID — client-side rejection, no network", () => {
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
              ZOMBIE_TOKEN: sessionJwt,
              ZOMBIE_API_URL: UNROUTABLE_API_URL,
              ZOMBIE_STATE_DIR: stateDir,
              NO_COLOR: "1",
            });
            const result = await runZombiectl([...row.args, sample, "--json"], { env: unroutable });
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

    // §3-residual — missing-required-arg sweep (moved here per HANDOFF.md;
    // CLI checks workspace-context before arg-validation, so this only
    // works inside the lifecycle suite which has state).
    describe("missing-required positional arg", () => {
      for (const row of REQUIRES_POSITIONAL_ARG) {
        it(`${row.args.join(" ")} (no <${row.missingArgName}>) exits non-zero`, async () => {
          const result = await expectMissingArg(row.args, env);
          assertNoSecretLeak(result, sessionJwt);
        });
      }
    });

    // Coverage check — every COMMAND_GROUP exercised somewhere in this suite
    // (workspace-wide read-only sweep + per-zombie sweep together cover
    // workspace/agent/grant/tenant/billing/zombie).
    it("touch every COMMAND_GROUP via the read-only sweep", () => {
      const exercised = new Set();
      for (const row of READ_ONLY_COMMANDS) {
        const head = row.args[0];
        if (head === "list" || head === "doctor") exercised.add("zombie");
        if (COMMAND_GROUPS.includes(head)) exercised.add(head);
      }
      for (const row of PER_ZOMBIE_READ_ONLY_COMMANDS) {
        if (row.group) exercised.add(row.group);
      }
      const missing = COMMAND_GROUPS.filter((g) => !exercised.has(g) && g !== "zombie");
      assert.deepEqual(missing, [], `command groups missing from §4b sweep: ${missing.join(",")}`);
    });
  });
}
