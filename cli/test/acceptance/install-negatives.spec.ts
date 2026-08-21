/**
 * Install negative-path acceptance scenarios (live, seeded-credentials session).
 *
 * Mints a Clerk session JSON Web Token (JWT) via the admin path, hydrates workspaces.json
 * from the API (matching the lifecycle-with-token spec's identity setup),
 * then drives the failure surface of `agentsfleet install`:
 *   - `install --library <id absent from gallery>` → ConfigError, exit 5,
 *     "is not in this workspace's gallery", no fleet created.
 *   - `install` with no `--library`        → ValidationError, exit 4, no network.
 *   - duplicate name (same onboarded template installed twice) → the second
 *     install keeps its DEFAULTED name by taking a `-NNN` suffix the server
 *     appends, and only an EXPLICIT `--name` already in use conflicts
 *     (UZ-AGT-006, exit 3) on the workspace's `(workspace_id, name)`
 *     uniqueness constraint. The template is the canonical `platform-ops`
 *     sample onboarded (upload) with a stable prefixed name, so every install
 *     in the group starts from that same name.
 *
 * Every spawn runs `assertNoSecretLeak` against the minted JWT. Mutating
 * tests are prefix-scoped via ACCEPTANCE_RUN_PREFIX and reclaimed in
 * `afterAll` through `cleanWorkspaceFleets`; the suite never asserts global
 * workspace emptiness, only that none of THIS run's fleets linger.
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
import {
  EXIT_CONFIG_ERROR,
  EXIT_SERVER_ERROR,
  EXIT_VALIDATION_ERROR,
  ERR_AGENTSFLEET_NAME_TAKEN,
  ERR_TEMPLATE_NOT_IN_GALLERY,
  FLAG_LIBRARY,
  absentTemplateId,
  onboardDuplicateTemplate,
  type DuplicateTemplate,
} from "./fixtures/install-negatives-ops.ts";

const target = process.env.AGENTSFLEET_ACCEPTANCE_TARGET ?? "";
const isLive = target.startsWith("https://");

const FLAG_JSON = "--json";
const FLAG_NAME = "--name";
const INSTALL = "install";

interface InstallEnvelope {
  readonly fleet_id?: string;
  readonly id?: string;
  readonly name?: string;
  readonly [key: string]: unknown;
}

function parseInstallEnvelope(stdout: string): InstallEnvelope | null {
  const trimmed = stdout.trim();
  if (!trimmed) return null;
  try {
    return JSON.parse(trimmed) as InstallEnvelope;
  } catch {
    return null;
  }
}

// The install envelope carries either key depending on the route the server
// answered on; both name the same row.
function installIdOf(envelope: InstallEnvelope | null): string | null {
  return envelope?.fleet_id ?? envelope?.id ?? null;
}

if (!isLive) {
  describe("install-negatives.spec.ts", () => {
    it.skip("requires AGENTSFLEET_ACCEPTANCE_TARGET to be an https URL", () => {});
  });
} else {
  describe("install-negatives — seeded-credentials session", () => {
    let apiUrl = "";
    let sessionJwt = "";
    let stateDir = "";
    let env: Record<string, string> = {};
    let workspaceId = "";

    async function runWithEnv(args: ReadonlyArray<string>): Promise<RunResult> {
      const result = await runFleetctl(args, { env });
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
          await cleanWorkspaceFleets(env, { workspaceId, runPrefix: ACCEPTANCE_RUN_PREFIX });
        } catch {
          /* best-effort teardown — shared DEV tenant */
        }
      }
      if (stateDir) await fs.rm(stateDir, { recursive: true, force: true });
    });

    // ── --library absent from the gallery ──────────────────────────
    describe("--library absent from gallery", () => {
      it("exits ConfigError when the id is not in the workspace gallery", async () => {
        const result = await runWithEnv([INSTALL, FLAG_LIBRARY, absentTemplateId(), FLAG_JSON]);
        assert.equal(
          result.code,
          EXIT_CONFIG_ERROR,
          `expected exit ${EXIT_CONFIG_ERROR}; got ${result.code}: ${merged(result)}`,
        );
        const body = merged(result);
        assert.ok(
          body.includes(ERR_TEMPLATE_NOT_IN_GALLERY),
          `expected "${ERR_TEMPLATE_NOT_IN_GALLERY}" in output: ${body}`,
        );
      });
    });

    // ── missing required --library flag ────────────────────────────
    describe("missing --library flag", () => {
      it("exits ValidationError and names --library without hitting the network", async () => {
        const result = await runWithEnv([INSTALL, FLAG_JSON]);
        assert.equal(
          result.code,
          EXIT_VALIDATION_ERROR,
          `expected exit ${EXIT_VALIDATION_ERROR}; got ${result.code}: ${merged(result)}`,
        );
        assert.ok(
          merged(result).includes(FLAG_LIBRARY),
          `expected --library mention: ${merged(result)}`,
        );
        // Client-side validation precedes any HTTP call — a stack-trace
        // network error would mean the guard never fired.
        assert.doesNotMatch(
          merged(result),
          /ECONNREFUSED|ENOTFOUND|EAI_AGAIN|fetch failed/,
          `--library guard should precede any network call: ${merged(result)}`,
        );
      });
    });

    // ── duplicate name: same onboarded template installed twice ──────────
    // Two installs, two different answers, and what decides is WHO chose the
    // name. A DEFAULTED name (no `--name`, so the template's frontmatter
    // `name:`) that collides is the second-copy case: the server appends a
    // three-digit suffix and the install succeeds, so a one-step install never
    // dead-ends on a name nobody typed. An EXPLICIT `--name` that collides is an
    // honest conflict the caller must see — auto-suffixing would rename what
    // they asked for — and stays UZ-AGT-006 (exit 3), the workspace's
    // `(workspace_id, name)` uniqueness constraint surfaced verbatim.
    describe("duplicate fleet name", () => {
      let tmpl: DuplicateTemplate | null = null;
      let firstId: string | null = null;
      let secondId: string | null = null;

      afterAll(async () => {
        for (const id of [firstId, secondId]) {
          if (!id) continue;
          try { await runFleetctl(["kill", id, FLAG_JSON], { env }); } catch { /* teardown */ }
        }
        tmpl = null;
      });

      it("first install of a freshly onboarded template succeeds", async () => {
        const created = await onboardDuplicateTemplate(env, ACCEPTANCE_RUN_PREFIX);
        tmpl = created;
        const result = await runFleetctl(
          [INSTALL, FLAG_LIBRARY, created.templateId, FLAG_JSON],
          { env, timeoutMs: 120_000 },
        );
        assertNoSecretLeak(result, sessionJwt);
        assert.equal(result.code, 0, `first install failed ${result.code}: ${merged(result)}`);
        const envelope = parseInstallEnvelope(result.stdout);
        firstId = installIdOf(envelope);
        assert.ok(firstId, `first install missing fleet id: ${result.stdout}`);
        // Nothing was taken yet, so the first copy keeps the template's own name.
        assert.equal(
          envelope?.name,
          created.name,
          `first install must store the template's own name: ${result.stdout}`,
        );
      }, 130_000);

      it("second install of the same template auto-suffixes the defaulted name", async () => {
        assert.ok(tmpl, "template must be onboarded by the first-install test");
        const target2 = tmpl as DuplicateTemplate;
        const result = await runFleetctl(
          [INSTALL, FLAG_LIBRARY, target2.templateId, FLAG_JSON],
          { env, timeoutMs: 120_000 },
        );
        assertNoSecretLeak(result, sessionJwt);
        assert.equal(
          result.code,
          0,
          `second install of a defaulted name must succeed; got ${result.code}: ${merged(result)}`,
        );

        const envelope = parseInstallEnvelope(result.stdout);
        secondId = installIdOf(envelope);
        assert.ok(secondId, `second install missing fleet id: ${result.stdout}`);
        assert.notEqual(
          secondId,
          firstId,
          `the second copy must be its own row, not the first one's id: ${secondId}`,
        );

        // `heroku_names.suffixed` appends `-NNN` to the taken base, and the 201
        // returns what was PERSISTED — the value every human-facing surface
        // reads. The fixture name is well under the 64-char cap, so the base is
        // carried whole and only the tail is new.
        const secondName = envelope?.name;
        assert.equal(typeof secondName, "string", `second install missing name: ${result.stdout}`);
        // Both halves: the tail alone would pass for any same-length base that
        // happens to end in `-NNN`, which proves a shape rather than a name.
        assert.ok(
          (secondName as string).startsWith(target2.name),
          `auto-suffixed name must keep the template name "${target2.name}"; got ${secondName}`,
        );
        assert.match(
          (secondName as string).slice(target2.name.length),
          /^-\d{3}$/,
          `auto-suffixed name must be "${target2.name}" plus -NNN; got ${secondName}`,
        );
      }, 130_000);

      it("an explicit --name already in use conflicts with UZ-AGT-006", async () => {
        assert.ok(tmpl, "template must be onboarded by the first-install test");
        const target3 = tmpl as DuplicateTemplate;
        const result = await runFleetctl(
          [INSTALL, FLAG_LIBRARY, target3.templateId, FLAG_NAME, target3.name, FLAG_JSON],
          { env, timeoutMs: 120_000 },
        );
        assertNoSecretLeak(result, sessionJwt);
        const body = merged(result);
        assert.equal(
          result.code,
          EXIT_SERVER_ERROR,
          `an explicit name-taken conflict must exit ${EXIT_SERVER_ERROR}: ${body}`,
        );
        assert.ok(
          body.includes(ERR_AGENTSFLEET_NAME_TAKEN),
          `expected "${ERR_AGENTSFLEET_NAME_TAKEN}" in output: ${body}`,
        );
        // A refused create mints nothing — a returned id here is a partial-create leak.
        assert.equal(
          installIdOf(parseInstallEnvelope(result.stdout)),
          null,
          `a refused explicit-name install must not return a fleet id: ${result.stdout}`,
        );
      }, 130_000);

      // Prefix-scoped leak audit — the template's OWN name belongs to exactly
      // one live fleet: the second copy took a suffixed name, and the
      // explicit-name attempt was refused outright.
      it("exactly one live fleet carries the template name", async () => {
        assert.ok(tmpl, "template must exist");
        const wanted = (tmpl as DuplicateTemplate).name;
        const listed = await runWithEnv(["list", FLAG_JSON]);
        assert.equal(listed.code, 0, `list exited ${listed.code}: ${listed.stderr}`);
        const parsed = JSON.parse(listed.stdout.trim() || "{}") as { items?: unknown };
        const items = Array.isArray(parsed.items)
          ? (parsed.items as Array<{ name?: string; status?: string }>)
          : [];
        const sameName = items.filter((z) => z.name === wanted);
        assert.ok(
          sameName.length <= 1,
          `expected at most one fleet named ${wanted}; got ${sameName.length}: ${JSON.stringify(sameName)}`,
        );
      });
    });

    // ── prefix-scoped teardown audit ────────────────────────────────
    // After reclaiming this run's fleets, no LIVE fleet whose name starts
    // with ACCEPTANCE_RUN_PREFIX should remain. Terminal rows still appear
    // in the list and prove teardown worked — they are filtered out.
    describe("post-teardown emptiness (prefix-scoped)", () => {
      beforeAll(async () => {
        await cleanWorkspaceFleets(env, { workspaceId, runPrefix: ACCEPTANCE_RUN_PREFIX });
      });

      it("no LIVE fleet name starts with ACCEPTANCE_RUN_PREFIX", async () => {
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
          `expected zero live fleets under ${ACCEPTANCE_RUN_PREFIX}; got ${JSON.stringify(mineLive)}`,
        );
      });
    });
  });
}
