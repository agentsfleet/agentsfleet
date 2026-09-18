/**
 * library-onboard-live — create a Fleet library from the terminal, then install it.
 *
 * The journey this grades is the one the CLI could not complete at all before
 * `library add` landed: a workspace gains a library, `agentsfleet library`
 * lists it, and `agentsfleet install --library <id>` accepts the identifier
 * that listing printed.
 *
 * # Why "the same identifier" is the assertion
 *
 * `library` used to read the platform-only catalogue while `install` resolved
 * the workspace gallery, so the two commands disagreed about what existed: a
 * tenant entry was installable and unlistable, and its identifier could only
 * come from the dashboard. Asserting that a freshly onboarded entry appears in
 * `library` output — and that `install` then takes it — is what pins the two
 * halves to one source.
 *
 * # Both real source kinds run
 *
 * `--from` uploads a bundle assembled here on disk; `--github` fetches a public
 * repository server-side. They exercise different daemon paths (inline body vs
 * tarball transport), and the walk would miss a transport regression if it took
 * only one.
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

const target = process.env[ACCEPTANCE_TARGET_ENV] ?? "";
const isLive = target.startsWith("https://");

const STATE_DIR_PREFIX = "agentsfleet-libadd-" as const;
const BUNDLE_DIR_PREFIX = "agentsfleet-bundle-" as const;
const NO_COLOR = "1" as const;
const JSON_FLAG = "--json" as const;
const SKILL_FILE = "SKILL.md" as const;
const TRIGGER_FILE = "TRIGGER.md" as const;
const TIER_TENANT = "tenant" as const;

// A public repository this organisation owns that carries SKILL.md at its root.
// Named rather than inlined because the github transport arm and its assertion
// both read it.
const PUBLIC_BUNDLE_REPO = "agentsfleet/github-pr-reviewer" as const;

const SETUP_TIMEOUT_MS = 120_000;
const ONBOARD_TIMEOUT_MS = 120_000;

/** A minimal bundle whose name carries the run prefix, so teardown finds it. */
const bundleName = (): string => `${ACCEPTANCE_RUN_PREFIX}libadd`;

const skillDocument = (name: string): string =>
  `---\nname: ${name}\ndescription: Acceptance probe for library onboarding.\n---\n# ${name}\n\nDoes nothing; exists to be onboarded.\n`;

const triggerDocument = (name: string): string =>
  `---\nname: ${name}\nx-agentsfleet:\n  tools:\n    - http_request\n  network:\n    allow:\n      - api.github.com\n---\n# Wake rule\n\nWoken by an explicit message only.\n`;

if (!isLive) {
  describe("library-onboard-live.spec.ts", () => {
    it.skip(`requires ${ACCEPTANCE_TARGET_ENV} to be an https URL`, () => {});
  });
} else {
  describe("library-onboard-live — onboard a library, list it, install from it", () => {
    let sessionJwt = "";
    let stateDir = "";
    let bundleDir = "";
    let env: Record<string, string> = {};
    let workspaceId = "";
    let uploadedLibraryId = "";
    let githubLibraryId = "";

    async function runWithEnv(args: ReadonlyArray<string>): Promise<RunResult> {
      const result = await runFleetctl(args, { env, timeoutMs: ONBOARD_TIMEOUT_MS });
      assertNoSecretLeak(result, sessionJwt);
      return result;
    }

    const parseJson = (out: string): Record<string, unknown> =>
      JSON.parse(out.trim()) as Record<string, unknown>;

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

      bundleDir = await fs.mkdtemp(path.join(os.tmpdir(), BUNDLE_DIR_PREFIX));
      const name = bundleName();
      await fs.writeFile(path.join(bundleDir, SKILL_FILE), skillDocument(name));
      await fs.writeFile(path.join(bundleDir, TRIGGER_FILE), triggerDocument(name));
    }, SETUP_TIMEOUT_MS);

    afterAll(async () => {
      if (env && workspaceId) {
        try {
          await cleanWorkspaceFleets(env, { workspaceId, runPrefix: ACCEPTANCE_RUN_PREFIX });
        } catch { /* best-effort teardown; never fail the run on cleanup */ }
      }
      if (bundleDir) await fs.rm(bundleDir, { recursive: true, force: true });
      if (stateDir) await fs.rm(stateDir, { recursive: true, force: true });
    });

    it("refuses an invocation naming no source, without reaching the daemon", async () => {
      const result = await runWithEnv(["library", "add"]);
      assert.equal(result.code, 4, `expected a usage rejection: ${result.stdout}${result.stderr}`);
      assert.ok(
        result.stderr.includes("--github"),
        `the refusal must name the three source flags: ${result.stderr}`,
      );
    });

    it("`library add --from` uploads a local bundle and returns a tenant entry", async () => {
      const result = await runWithEnv(["library", "add", "--from", bundleDir, JSON_FLAG]);
      assert.equal(result.code, 0, `library add --from failed: ${result.stderr}`);
      const created = parseJson(result.stdout);
      assert.equal(typeof created.id, "string", `no library id returned: ${result.stdout}`);
      assert.equal(created.visibility, TIER_TENANT,
        `an onboarded workspace library is a tenant entry: ${result.stdout}`);
      uploadedLibraryId = created.id as string;
    }, ONBOARD_TIMEOUT_MS);

    it("`library add --github` fetches a public repository server-side", async () => {
      const result = await runWithEnv([
        "library", "add", "--github", PUBLIC_BUNDLE_REPO, JSON_FLAG,
      ]);
      assert.equal(result.code, 0, `library add --github failed: ${result.stderr}`);
      const created = parseJson(result.stdout);
      assert.equal(typeof created.id, "string", `no library id returned: ${result.stdout}`);
      assert.equal(created.visibility, TIER_TENANT);
      githubLibraryId = created.id as string;
    }, ONBOARD_TIMEOUT_MS);

    it("`library` lists both onboarded entries, each carrying its tier", async () => {
      assert.ok(uploadedLibraryId && githubLibraryId, "nothing was onboarded to list");
      const result = await runWithEnv(["library", JSON_FLAG]);
      assert.equal(result.code, 0, `library failed: ${result.stderr}`);
      const listed = parseJson(result.stdout) as {
        items?: Array<{ id?: string; visibility?: string }>;
      };
      const ids = (listed.items ?? []).map((row) => row.id);
      // The whole point: what `library` prints is what `install` will accept.
      assert.ok(ids.includes(uploadedLibraryId),
        `the uploaded entry is missing from the gallery: ${result.stdout}`);
      assert.ok(ids.includes(githubLibraryId),
        `the github entry is missing from the gallery: ${result.stdout}`);
      assert.ok((listed.items ?? []).every((row) => typeof row.visibility === "string"),
        `every row carries a tier: ${result.stdout}`);
    });

    it("`install --library` accepts the identifier the listing printed", async () => {
      assert.ok(uploadedLibraryId, "nothing was onboarded to install");
      const result = await runWithEnv([
        "install", "--library", uploadedLibraryId, "--name", `${ACCEPTANCE_RUN_PREFIX}libadd-fleet`, JSON_FLAG,
      ]);
      assert.equal(result.code, 0, `install failed: ${result.stderr}`);
      const installed = parseJson(result.stdout);
      assert.equal(typeof installed.fleet_id, "string",
        `install returned no fleet id: ${result.stdout}`);
    }, ONBOARD_TIMEOUT_MS);
  });
}
