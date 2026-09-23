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
 * walk needs no network and no repository: `library create --from` uploads
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
import { CLI_KEY_PREFIX, cleanWorkspaceLibraryEntries } from "./fixtures/teardown";
import { gotoWorkspace, workspaceHref, workspaceUrlPattern } from "./fixtures/nav";
import {
  cliEnv,
  deleteCliKey,
  makeCliStateDir,
  mintCliKey,
  spawnAgentsfleet,
  writeCliState,
} from "./fixtures/cli-runner";

const SOURCE_KIND_UPLOAD = "upload";
const LIBRARY_SUBPATH = "library";
const GALLERY_SUBPATH = "fleets/new";
/**
 * The workspace entry, addressed by where it GOES rather than what it says.
 *
 * It shares its label with the platform catalogue's entry under /admin. A
 * comment here once argued that the regular fixture never sees that one, so the
 * name was unambiguous — the acceptance run disproved it, and matching on the
 * name is the wrong instinct regardless: the destination is what this click is
 * for, and it is the only thing that separates the two.
 */
const WORKSPACE_NAV_LINK = `a[href$="/${LIBRARY_SUBPATH}"]`;
/** The row action is a glyph; this is the tooltip and its accessible name. */
const REMOVE_ROW_LABEL = "Remove from this workspace";
/** The dialog's confirm, which is the bare verb. */
const REMOVE_LABEL = "Remove";

/** How long the confirmation may take to appear after one click. */
const DIALOG_OPEN_MS = 3_000;

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
    await page.locator(WORKSPACE_NAV_LINK).click();
    await expect(page).toHaveURL(workspaceUrlPattern(LIBRARY_SUBPATH));

    const row = page.getByRole("row").filter({ hasText: name });
    await expect(row).toBeVisible();

    // One click, and the question opens. This used to need a retry: the row
    // action read `disabled={pending}` from the transition Load more shares,
    // so a click landing during a page fetch was dropped and the dialog never
    // came. Retrying made the test pass and left the person clicking twice, so
    // the button stopped being disabled instead — opening the question sends
    // nothing, and there was never a request here to guard.
    const removeAction = row.getByRole("button", { name: REMOVE_ROW_LABEL });
    await expect(removeAction).toBeEnabled();
    await removeAction.click();

    // The confirm is addressed through the dialog rather than by label alone:
    // the row action's name now contains the verb, so a bare "Remove" would
    // still be ambiguous across the page.
    // `alertdialog`, not `dialog` — ConfirmDialog sets `role="alertdialog"`
    // (design-system/ConfirmDialog.tsx:88), so `getByRole("dialog")` matches
    // nothing. The sibling platform spec already addresses it correctly; this
    // one never ran long enough to find out, because the lane died in global
    // setup on the commits before this.
    const dialog = page.getByRole("alertdialog");
    await expect(dialog).toBeVisible({ timeout: DIALOG_OPEN_MS });
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
    let minted: { key: string; id: string } | null = null;

    try {
      // An `agt_t` key, not the session JWT: the CLI refuses a JWT on shape
      // alone and reports it as "not authenticated".
      minted = await mintCliKey(
        apiUrl,
        sessionJwtFor(FIXTURE_KEY.regular),
        `${CLI_KEY_PREFIX}${Math.random().toString(36).slice(2, 8)}`,
      );
      await writeCliState(stateDir, ws, minted.key, apiUrl, WORKSPACE_NAME);
      const env = cliEnv({ AGENTSFLEET_STATE_DIR: stateDir, AGENTSFLEET_API_URL: apiUrl });
      const bundle = await writeBundle(root, name);

      const added = await spawnAgentsfleet(["--json", "library", "create", "--from", bundle], env);
      expect(added.code, `library create failed:\n${added.stdout}\n${added.stderr}`).toBe(0);
      const entryId = (JSON.parse(added.stdout) as { id?: string }).id;
      expect(entryId).toBeTruthy();

      const listed = await spawnAgentsfleet(["--json", "library", "list"], env);
      expect(listed.code, listed.stderr).toBe(0);
      expect(listed.stdout).toContain(entryId);

      const removed = await spawnAgentsfleet(["library", "delete", String(entryId)], env);
      expect(removed.code, `library delete failed:\n${removed.stdout}\n${removed.stderr}`).toBe(0);
      expect(removed.stdout).toContain(String(entryId));

      const after = await spawnAgentsfleet(["--json", "library", "list"], env);
      expect(after.code, after.stderr).toBe(0);
      expect(after.stdout).not.toContain(entryId);
    } finally {
      // A leaked API key is a live credential, so it goes even if the walk threw.
      if (minted) {
        await deleteCliKey(apiUrl, sessionJwtFor(FIXTURE_KEY.regular), minted.id);
      }
      await fs.rm(root, { recursive: true, force: true });
    }
  });

  test.afterEach(async () => {
    const ws = await getDefaultWorkspaceId(FIXTURE_KEY.regular);
    await cleanWorkspaceLibraryEntries(FIXTURE_KEY.regular, ws);
  });
});
