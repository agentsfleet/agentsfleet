/**
 * workspace-library.spec.ts — listing and removing what a workspace onboarded.
 *
 * Two walks over the verb this workstream adds, one per client.
 *
 * The browser walk reaches the page the way an operator does — through the
 * sidebar, not a typed URL — because "the page exists" and "the page is
 * reachable" are different claims and only the second is what shipped.
 *
 * The command-line walk runs the REAL binary in a subprocess against local
 * `agentsfleetd`. It onboards from a bundle directory it writes itself, so the
 * walk needs no network and no repository: `library add --from` uploads
 * SKILL.md and TRIGGER.md and nothing else.
 *
 * Both onboard under a per-run name. The gallery converges identical bytes
 * onto one row per workspace, so a shared name across concurrent runs would
 * have one run removing the row another is asserting on.
 */
import * as fs from "node:fs/promises";
import * as fsSync from "node:fs";
import * as path from "node:path";
import { expect, test } from "@playwright/test";
import { signInAs } from "./fixtures/auth";
import { FIXTURE_KEY } from "./fixtures/constants";
import { getDefaultWorkspaceId, skillMd, triggerMd } from "./fixtures/seed";
import { cleanWorkspaceLibraryEntries } from "./fixtures/teardown";
import { gotoWorkspace, workspaceHref, workspaceUrlPattern } from "./fixtures/nav";
import { cliEnv, makeCliStateDir, spawnAgentsfleet, writeCliState } from "./fixtures/cli-runner";

const SOURCE_KIND_UPLOAD = "upload";
const LIBRARY_SUBPATH = "library";
const GALLERY_SUBPATH = "fleets/new";
/** The sidebar entry, which is "Library" and not "Fleet library" — that label
 * belongs to the platform catalogue under /admin. */
const SIDEBAR_LABEL = "Library";
const REMOVE_LABEL = "Remove";
const BUNDLE_SKILL_FILE = "SKILL.md";
const BUNDLE_TRIGGER_FILE = "TRIGGER.md";
const WORKSPACE_NAME = "fixture-workspace";
const CLI_STATE_PREFIX = "library-cli-";

/** A name no other run is using, short enough to read in a failure. */
function uniqueName(prefix: string): string {
  return `${prefix}-${Math.random().toString(36).slice(2, 8)}`;
}

function sessionJwtFor(key: string): string {
  const cachePath = path.join(process.cwd(), ".fixture-jwts.json");
  const cache = JSON.parse(fsSync.readFileSync(cachePath, "utf8")) as Record<
    string,
    { sessionJwt?: string }
  >;
  const token = cache[key]?.sessionJwt;
  if (!token) throw new Error(`fixture cache missing sessionJwt for '${key}'`);
  return token;
}

function apiUrlOrThrow(): string {
  const apiUrl = process.env.NEXT_PUBLIC_API_URL;
  if (!apiUrl) throw new Error("NEXT_PUBLIC_API_URL must be set");
  return apiUrl;
}

/** Onboards one upload bundle over the shipped endpoint and answers its id. */
async function onboardLibrary(
  apiUrl: string,
  workspaceId: string,
  token: string,
  name: string,
): Promise<string> {
  const response = await fetch(
    `${apiUrl}/v1/workspaces/${encodeURIComponent(workspaceId)}/fleet-libraries`,
    {
      method: "POST",
      headers: { Authorization: `Bearer ${token}`, "Content-Type": "application/json" },
      body: JSON.stringify({
        source_kind: SOURCE_KIND_UPLOAD,
        skill_markdown: skillMd(name),
        trigger_markdown: triggerMd(name),
      }),
    },
  );
  if (!response.ok) {
    throw new Error(`library onboard failed (${response.status}): ${await response.text()}`);
  }
  const body = (await response.json()) as { id?: unknown };
  if (typeof body.id !== "string" || body.id.length === 0) {
    throw new Error("library onboard returned no id");
  }
  return body.id;
}

/** A bundle directory holding exactly the two documents an upload carries. */
async function writeBundle(root: string, name: string): Promise<string> {
  const bundle = path.join(root, "bundle");
  await fs.mkdir(bundle, { recursive: true });
  await fs.writeFile(path.join(bundle, BUNDLE_SKILL_FILE), skillMd(name));
  await fs.writeFile(path.join(bundle, BUNDLE_TRIGGER_FILE), triggerMd(name));
  return bundle;
}

test.describe("workspace-library", () => {
  test("test_workspace_library_page_walk", async ({ page }) => {
    const apiUrl = apiUrlOrThrow();
    const ws = await getDefaultWorkspaceId(FIXTURE_KEY.regular);
    const name = uniqueName("library-walk");
    await onboardLibrary(apiUrl, ws, sessionJwtFor(FIXTURE_KEY.regular), name);

    await signInAs(page, FIXTURE_KEY.regular);
    await gotoWorkspace(page, FIXTURE_KEY.regular);

    // Reached from the navigation, not a typed URL: a page nothing links to
    // is a page the operator never finds.
    await page.getByRole("link", { name: SIDEBAR_LABEL, exact: true }).click();
    await expect(page).toHaveURL(workspaceUrlPattern(LIBRARY_SUBPATH));

    const row = page.getByRole("row").filter({ hasText: name });
    await expect(row).toBeVisible();

    await row.getByRole("button", { name: REMOVE_LABEL }).click();
    // Both the row action and the dialog's confirm read "Remove", so the
    // confirm is addressed through the dialog rather than by label alone.
    const dialog = page.getByRole("dialog");
    await expect(dialog).toContainText(name);
    await dialog.getByRole("button", { name: REMOVE_LABEL }).click();

    await expect(row).toHaveCount(0);

    // And it is gone from the place it was installable from.
    await page.goto(workspaceHref(ws, GALLERY_SUBPATH));
    await expect(page).toHaveURL(workspaceUrlPattern(GALLERY_SUBPATH));
    const card = page.getByRole("article").filter({ hasText: name });
    await expect(card).toHaveCount(0);
  });

  test("test_library_remove_subprocess_walk", async () => {
    const apiUrl = apiUrlOrThrow();
    const ws = await getDefaultWorkspaceId(FIXTURE_KEY.regular);
    const name = uniqueName("library-cli");
    const { root, stateDir } = await makeCliStateDir(CLI_STATE_PREFIX);

    try {
      await writeCliState(stateDir, ws, sessionJwtFor(FIXTURE_KEY.regular), apiUrl, WORKSPACE_NAME);
      const env = cliEnv({ AGENTSFLEET_STATE_DIR: stateDir, AGENTSFLEET_API_URL: apiUrl });
      const bundle = await writeBundle(root, name);

      const added = await spawnAgentsfleet(["--json", "library", "add", "--from", bundle], env);
      expect(added.code, `add failed:\n${added.stdout}\n${added.stderr}`).toBe(0);
      const entryId = (JSON.parse(added.stdout) as { id?: string }).id;
      expect(entryId).toBeTruthy();

      const listed = await spawnAgentsfleet(["--json", "library", "list"], env);
      expect(listed.code, listed.stderr).toBe(0);
      expect(listed.stdout).toContain(entryId);

      const removed = await spawnAgentsfleet(["library", "remove", String(entryId)], env);
      expect(removed.code, `remove failed:\n${removed.stdout}\n${removed.stderr}`).toBe(0);
      expect(removed.stdout).toContain(String(entryId));

      const after = await spawnAgentsfleet(["--json", "library", "list"], env);
      expect(after.code, after.stderr).toBe(0);
      expect(after.stdout).not.toContain(entryId);
    } finally {
      await fs.rm(root, { recursive: true, force: true });
    }
  });

  test.afterEach(async () => {
    const ws = await getDefaultWorkspaceId(FIXTURE_KEY.regular);
    await cleanWorkspaceLibraryEntries(FIXTURE_KEY.regular, ws);
  });
});
