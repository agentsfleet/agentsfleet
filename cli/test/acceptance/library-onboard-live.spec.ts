/**
 * library-onboard-live — create a Fleet library from the terminal, then install it.
 *
 * The journey this grades is the one the CLI could not complete at all before
 * `library create` landed: a workspace gains a library, `agentsfleet library`
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
import { cleanWorkspaceFleets, cleanWorkspaceLibraryEntries } from "./fixtures/teardown.ts";

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

/** How long the gallery may take to show a row `library create` just returned. */
const LISTING_SETTLE_MS = 30_000;

/** The gap between gallery reads while waiting for that row. */
const LISTING_POLL_MS = 1_000;

/** How much of a listing a refusal quotes before a CI log stops being readable. */
const STDOUT_EXCERPT = 2_000;

/**
 * Why an identifier is not in the gallery, in one sentence.
 *
 * Distinguishes the two failures that read identically from a raw dump: the
 * entry is absent entirely, or it is present under a different identifier than
 * the one `library create` returned. The second is a defect in the daemon; the
 * first is a timing or scope problem, and they are fixed in different places.
 *
 * Matched on the entry's OWN name, not on the run prefix. Only the uploaded
 * bundle is named after the run — the GitHub entry takes its name from the
 * repository's own frontmatter — so a prefix match answers for the wrong row:
 * with the upload present and the GitHub entry missing it would report an
 * identifier mismatch that had not happened, and with the upload missing it
 * would report nothing from this run while a GitHub row sat in the listing.
 */
const missingFrom = (
  which: string,
  wanted: string,
  expectedName: string,
  listed: { items?: Array<{ id?: string; name?: string }> },
  stdout: string,
): string => {
  const items = listed.items ?? [];
  const sameName = items.filter((row) => row.name === expectedName);
  const named = sameName.map((row) => `${row.name}=${row.id}`).join(", ") || "none";
  return (
    `the ${which} entry is missing from the gallery. ` +
    `wanted name=${expectedName} id=${wanted}; gallery holds ${items.length} row(s); ` +
    `rows under that name: ${named}. ` +
    (sameName.length > 0
      ? "That name IS listed, so the identifier the gallery prints differs from the one create returned."
      : `That name is not listed at all. stdout: ${stdout.slice(0, STDOUT_EXCERPT)}`)
  );
};

/**
 * The ONE top-level object a `--json` command printed, or a loud failure.
 *
 * `trailingJson` walks backward from the last `}` to its matching `{`, which is
 * exactly right for `steer --json`: prose, then one small object. It is wrong
 * here, and wrong in a way that reads as a product bug.
 *
 * A gallery listing is an envelope wrapping N rows. Truncate that text anywhere
 * mid-array and the last `}` stops belonging to the envelope and starts
 * belonging to a ROW — so the backward walk returns that row, it parses
 * cleanly, `items` is undefined, and the assertion reports "gallery holds 0
 * row(s) ... That name is not listed at all" directly above a stdout excerpt
 * containing that very name. The message and its own evidence disagree, and
 * whoever reads it goes looking for a row that was never missing.
 *
 * `JSON.parse` is the whole implementation. It already rejects a truncated
 * document and trailing garbage, which is the entire property wanted here —
 * a scanner written by hand to find "the right brace" is a second parser to
 * get wrong. The one thing it cannot do is skip prose the CLI printed first,
 * and that is an `indexOf` away.
 */
function envelopeJson(stdout: string, label: string): Record<string, unknown> {
  const open = stdout.indexOf("{");
  assert.ok(open >= 0, `${label}: no JSON object in stdout: ${stdout.slice(0, STDOUT_EXCERPT)}`);
  try {
    return JSON.parse(stdout.slice(open)) as Record<string, unknown>;
  } catch (cause) {
    throw new assert.AssertionError({
      message:
        `${label}: stdout is not one complete JSON object — ` +
        `${stdout.length} byte(s) captured, so it is truncated or followed by a second ` +
        `document, NOT an empty gallery. ${String(cause)}. ` +
        `Tail: ${stdout.slice(-STDOUT_EXCERPT)}`,
    });
  }
}

/** A minimal bundle whose name carries the run prefix, so teardown finds it. */
const bundleName = (): string => `${ACCEPTANCE_RUN_PREFIX}libadd`;

// Minimal means minimal FOR THE DAEMON, not for a reader. `afd_library`'s
// `frontmatter::skill` deserialises name + description + `version` and rejects
// a document missing any of the three, and `FleetConfig::authored` reports
// `MissingRequiredField` for `triggers` and for `budget`. A document short of
// one of those is refused at onboard with UZ-BUNDLE-001 — whose sentence says
// "missing SKILL.md, or has an unsafe or oversized file" and names none of
// them — so the five fields below are each load-bearing.
const skillDocument = (name: string): string =>
  `---\nname: ${name}\ndescription: Acceptance probe for library onboarding.\nversion: 0.1.0\n---\n# ${name}\n\nDoes nothing; exists to be onboarded.\n`;

// `type: api` is the wake this probe wants — woken by an authenticated call and
// by nothing else — and it is also the only trigger variant that carries no
// configuration of its own, so the document declares a wake without declaring a
// repository this fleet has no business reaching.
const triggerDocument = (name: string): string =>
  `---\nname: ${name}\nx-agentsfleet:\n  triggers:\n    - type: api\n  tools:\n    - http_request\n  network:\n    allow:\n      - api.github.com\n  budget:\n    daily_dollars: 1.0\n---\n# Wake rule\n\nWoken by an explicit message only.\n`;

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
    // The names the daemon stored, read back from what create answered rather
    // than assumed here. The uploaded bundle is named after the run; the
    // GitHub entry is named by the repository's own frontmatter, and only the
    // daemon knows which of the two a given row is.
    let uploadedEntryName = "";
    let githubEntryName = "";

    async function runWithEnv(args: ReadonlyArray<string>): Promise<RunResult> {
      const result = await runFleetctl(args, { env, timeoutMs: ONBOARD_TIMEOUT_MS });
      assertNoSecretLeak(result, sessionJwt);
      return result;
    }

    // The lane's own reader, plus the two streams named when there is nothing
    // to read. A bare `JSON.parse(stdout)` threw `Unexpected EOF` on an empty
    // gallery read and reported neither the exit code nor one byte of what the
    // CLI wrote, so the failure said only that the string was not JSON. Every
    // sibling spec in this lane reads through `trailingJson`, which suits a
    // steer envelope and not a listing — see `envelopeJson` above.
    const parseJson = (result: RunResult, label: string): Record<string, unknown> => {
      assert.ok(
        result.stdout.trim().length > 0,
        `${label}: exited ${result.code} and wrote no stdout; stderr: ${result.stderr}`,
      );
      return envelopeJson(result.stdout, label);
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

      bundleDir = await fs.mkdtemp(path.join(os.tmpdir(), BUNDLE_DIR_PREFIX));
      const name = bundleName();
      await fs.writeFile(path.join(bundleDir, SKILL_FILE), skillDocument(name));
      await fs.writeFile(path.join(bundleDir, TRIGGER_FILE), triggerDocument(name));
    }, SETUP_TIMEOUT_MS);

    afterAll(async () => {
      if (env && workspaceId) {
        try {
          // Fleets first, then the entries they were installed from: a fleet
          // holds its own copy of the bundle, so the order is not a constraint
          // — but reading the gallery after the fleets are gone keeps the
          // listing this walks small.
          await cleanWorkspaceFleets(env, { workspaceId, runPrefix: ACCEPTANCE_RUN_PREFIX });
          await cleanWorkspaceLibraryEntries(env, {
            workspaceId,
            runPrefix: ACCEPTANCE_RUN_PREFIX,
          });
        } catch { /* best-effort teardown; never fail the run on cleanup */ }
      }
      if (bundleDir) await fs.rm(bundleDir, { recursive: true, force: true });
      if (stateDir) await fs.rm(stateDir, { recursive: true, force: true });
    });

    it("refuses an invocation naming no source, without reaching the daemon", async () => {
      const result = await runWithEnv(["library", "create"]);
      assert.equal(result.code, 4, `expected a usage rejection: ${result.stdout}${result.stderr}`);
      assert.ok(
        result.stderr.includes("--github"),
        `the refusal must name the three source flags: ${result.stderr}`,
      );
    });

    it("`library create --from` uploads a local bundle and returns a tenant entry", async () => {
      const result = await runWithEnv(["library", "create", "--from", bundleDir, JSON_FLAG]);
      assert.equal(result.code, 0, `library create --from failed: ${result.stderr}`);
      const created = parseJson(result, "library create --from");
      assert.equal(typeof created.id, "string", `no library id returned: ${result.stdout}`);
      assert.equal(created.visibility, TIER_TENANT,
        `an onboarded workspace library is a tenant entry: ${result.stdout}`);
      uploadedLibraryId = created.id as string;
      uploadedEntryName = created.name as string;
    }, ONBOARD_TIMEOUT_MS);

    it("`library create --github` fetches a public repository server-side", async () => {
      const result = await runWithEnv([
        "library", "create", "--github", PUBLIC_BUNDLE_REPO, JSON_FLAG,
      ]);
      assert.equal(result.code, 0, `library create --github failed: ${result.stderr}`);
      const created = parseJson(result, "library create --github");
      assert.equal(typeof created.id, "string", `no library id returned: ${result.stdout}`);
      assert.equal(created.visibility, TIER_TENANT);
      githubLibraryId = created.id as string;
      githubEntryName = created.name as string;
    }, ONBOARD_TIMEOUT_MS);

    it("`library` lists both onboarded entries, each carrying its tier", async () => {
      assert.ok(uploadedLibraryId && githubLibraryId, "nothing was onboarded to list");

      // Polled, not read once. The gallery is a live distributed read behind a
      // write that just returned, and a single read asserts that the two are
      // instantaneous. They are not required to be, and in CI they are not: this
      // case has failed there while passing locally against the same deployment
      // and the same collection, which is the signature of a read that ran too
      // early rather than a gallery missing a row.
      const deadline = Date.now() + LISTING_SETTLE_MS;
      let listed: { items?: Array<{ id?: string; name?: string; visibility?: string }> } = {};
      let ids: Array<string | undefined> = [];
      let stdout = "";
      for (;;) {
        const result = await runWithEnv(["library", JSON_FLAG]);
        assert.equal(result.code, 0, `library failed: ${result.stderr}`);
        stdout = result.stdout;
        listed = parseJson(result, "library --json") as typeof listed;
        ids = (listed.items ?? []).map((row) => row.id);
        if (ids.includes(uploadedLibraryId) && ids.includes(githubLibraryId)) break;
        if (Date.now() >= deadline) break;
        await new Promise((settle) => setTimeout(settle, LISTING_POLL_MS));
      }

      // The whole point: what `library` prints is what `install` will accept.
      //
      // The message carries the identifiers and the name match rather than the
      // whole listing. A gallery of several hundred rows scrolls the reason off
      // the top of a CI log, and "is the row absent, or present under another
      // id" is the question a reader actually has.
      assert.ok(
        ids.includes(uploadedLibraryId),
        missingFrom("uploaded", uploadedLibraryId, uploadedEntryName, listed, stdout),
      );
      assert.ok(
        ids.includes(githubLibraryId),
        missingFrom("github", githubLibraryId, githubEntryName, listed, stdout),
      );
      assert.ok((listed.items ?? []).every((row) => typeof row.visibility === "string"),
        `every row carries a tier: ${stdout}`);
    });

    it("`install --library` accepts the identifier the listing printed", async () => {
      assert.ok(uploadedLibraryId, "nothing was onboarded to install");
      const result = await runWithEnv([
        "install", "--library", uploadedLibraryId, "--name", `${ACCEPTANCE_RUN_PREFIX}libadd-fleet`, JSON_FLAG,
      ]);
      assert.equal(result.code, 0, `install failed: ${result.stderr}`);
      const installed = parseJson(result, "install --library");
      assert.equal(typeof installed.fleet_id, "string",
        `install returned no fleet id: ${result.stdout}`);
    }, ONBOARD_TIMEOUT_MS);
  });
}
