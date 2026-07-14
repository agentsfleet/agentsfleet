/**
 * platform-library-onboarding.spec.ts — a platform operator runs the whole fleet
 * catalog from the dashboard, and nobody else can.
 *
 * The catalog is runtime data (M128): no migration seeds a fleet, so this surface
 * is the ONLY way one comes to exist. An operator adds it from a repository and it
 * lands as a draft that no workspace can see. Publishing is a separate, deliberate
 * act, and it is the only door to a tenant.
 *
 * Four claims, none of which a unit test can make:
 *   - the scope actually gates the routes end-to-end (real Clerk session, real
 *     agentsfleetd `requireScope`), not just the mocked `hasScope`;
 *   - a bad repository surfaces the importer's real UZ error in the dialog;
 *   - a real GitHub import lands a real row — as a DRAFT, invisible to a real
 *     workspace, until a real publish;
 *   - unpublishing takes it back out of that workspace's gallery.
 *
 * The operator fixture is the only one whose Clerk `public_metadata.scopes`
 * carries `platform-library:write` (fixtures/constants.ts). The regular fixture is
 * deliberately scope-free, which is what makes the negative case meaningful.
 */
import { expect, test, type Page } from "@playwright/test";
import { signInAs } from "./fixtures/auth";
import { workspaceUrlPattern } from "./fixtures/nav";
import { FIXTURE_KEY } from "./fixtures/constants";

const ADMIN_PATH = "/admin/fleet-libraries";
const NAV_LABEL = "Fleet library";

// Published at agentsfleet/<id>. The catalog id is the bundle's SKILL.md
// frontmatter name — not the repository path the operator types.
const SAMPLE_REPO = "agentsfleet/platform-ops";
const SAMPLE_ENTRY_ID = "platform-ops";

// A repository that does not exist, to drive the importer's fetch failure into the
// dialog rather than a crash or a silent close.
const MISSING_REPO = "agentsfleet/definitely-not-a-fleet-bundle";

const IMPORT_TIMEOUT = 60_000;

// Add the sample fleet. Idempotent by design: re-adding the SAME repository is the
// refetch path, not a collision, so a re-run of this suite is safe.
async function addSampleFleet(page: Page) {
  await page.goto(ADMIN_PATH);
  await page.getByRole("button", { name: /create fleet library/i }).click();
  await page.getByLabel(/repository/i).fill(SAMPLE_REPO);
  await page.getByRole("button", { name: /^create fleet library$/i }).click();
  await expect(page.getByText(SAMPLE_ENTRY_ID)).toBeVisible({ timeout: IMPORT_TIMEOUT });
}

function galleryCards(page: Page) {
  return page.getByTestId(`library-card-${SAMPLE_ENTRY_ID}`);
}

test.describe("platform fleet catalog", () => {
  test("a workspace user never sees the operator surface", async ({ page }) => {
    await signInAs(page, FIXTURE_KEY.regular);
    await page.goto("/");

    await expect(page.getByRole("link", { name: NAV_LABEL })).toHaveCount(0);

    // Even by direct URL: the page redirects rather than rendering an action the
    // session could not take.
    await page.goto(ADMIN_PATH);
    await expect(page).not.toHaveURL(new RegExp(ADMIN_PATH));
  });

  test("an operator reaches the surface from the nav", async ({ page }) => {
    await signInAs(page, FIXTURE_KEY.operator);
    await page.goto("/");

    await page.getByRole("link", { name: NAV_LABEL }).click();

    await expect(page).toHaveURL(new RegExp(ADMIN_PATH));
    await expect(page.getByRole("heading", { name: NAV_LABEL })).toBeVisible();
  });

  test("a repository that cannot be imported keeps the dialog open with the error", async ({
    page,
  }) => {
    await signInAs(page, FIXTURE_KEY.operator);
    await page.goto(ADMIN_PATH);

    await page.getByRole("button", { name: /create fleet library/i }).click();
    await page.getByLabel(/repository/i).fill(MISSING_REPO);
    await page.getByRole("button", { name: /^create fleet library$/i }).click();

    // The dialog stays mounted with the failure shown — the operator corrects the
    // repository in place rather than losing what they typed.
    await expect(page.getByRole("alert")).toBeVisible({ timeout: 30_000 });
    await expect(page.getByLabel(/repository/i)).toBeVisible();
  });

  // The heart of the milestone. A fleet an operator has added is NOT live: the
  // publish gate is what protects every tenant from an unreviewed bundle, and it is
  // worthless unless a draft is genuinely unreachable.
  test("an added fleet is a draft no workspace can see until it is published", async ({ page }) => {
    await signInAs(page, FIXTURE_KEY.operator);
    await addSampleFleet(page);

    // It exists, and it is a draft. The table says so.
    await expect(page.getByText("Draft")).toBeVisible();

    // A plain workspace user cannot see it. Not hidden-but-installable — absent.
    await signInAs(page, FIXTURE_KEY.regular);
    await page.goto("/");
    await expect(page).toHaveURL(workspaceUrlPattern());
    const workspacePath = new URL(page.url()).pathname;
    await page.goto(`${workspacePath}/fleets/new`);
    await expect(galleryCards(page)).toHaveCount(0);

    // The operator publishes. This is the only act that opens the door.
    await signInAs(page, FIXTURE_KEY.operator);
    await page.goto(ADMIN_PATH);
    await page.getByRole("button", { name: /^publish$/i }).click();
    await expect(page.getByText("Published")).toBeVisible({ timeout: 30_000 });

    // Now the same workspace user can install it — exactly once. The gallery is
    // where a duplicate catalog row would show, so this also pins that re-adding
    // upserts rather than minting a second entry.
    await signInAs(page, FIXTURE_KEY.regular);
    await page.goto(`${workspacePath}/fleets/new`);
    await expect(galleryCards(page).first()).toBeVisible({ timeout: 30_000 });
    await expect(galleryCards(page)).toHaveCount(1);

    // And withdrawing takes it back out. Unpublish is a real withdrawal, not a
    // cosmetic flag: the fleet leaves the gallery it was installable from.
    await signInAs(page, FIXTURE_KEY.operator);
    await page.goto(ADMIN_PATH);
    await page.getByRole("button", { name: /^unpublish$/i }).click();
    await expect(page.getByText("Draft")).toBeVisible({ timeout: 30_000 });

    await signInAs(page, FIXTURE_KEY.regular);
    await page.goto(`${workspacePath}/fleets/new`);
    await expect(galleryCards(page)).toHaveCount(0);
  });

  // The pencil: the install-gate copy is the platform's voice, and the operator owns
  // it. A bundle refetch must never undo what they wrote.
  test("the operator's install-gate copy survives a bundle refetch", async ({ page }) => {
    const COPY = "Reads your Fly.io app state to diagnose the incident.";

    await signInAs(page, FIXTURE_KEY.operator);
    await addSampleFleet(page);

    await page.getByRole("button", { name: /^edit$/i }).click();
    await page.getByLabel(/^description$/i).fill(COPY);
    await page.getByRole("button", { name: /^save$/i }).click();
    await expect(page.getByLabel(/^description$/i)).toHaveCount(0, { timeout: 30_000 });

    // Re-fetch the bundle from the same repository — the update path.
    await page.getByRole("button", { name: /fetch update/i }).click();
    await page.getByRole("button", { name: /^create fleet library$/i }).click();
    await expect(page.getByText(SAMPLE_ENTRY_ID)).toBeVisible({ timeout: IMPORT_TIMEOUT });

    // The operator's copy is still there. The server keeps `description` out of the
    // refetch upsert precisely so this holds (M128 Invariant 4).
    await page.getByRole("button", { name: /^edit$/i }).click();
    await expect(page.getByLabel(/^description$/i)).toHaveValue(COPY);
  });

  // M130 — the recovery path the milestone exists for. A mistyped repository is
  // corrected IN PLACE: the repoint discards the stored bundle and withdraws the
  // row (a fleet must never advertise a source it is not serving), then a refetch
  // and republish bring it back — with the operator's curated copy intact the
  // whole way, because none of this ever deleted the row.
  test("the operator corrects a mistyped repository in place and the fleet returns", async ({
    page,
  }) => {
    await signInAs(page, FIXTURE_KEY.operator);
    await addSampleFleet(page);
    await page.getByRole("button", { name: /^publish$/i }).click();
    await expect(page.getByText("Published")).toBeVisible({ timeout: 30_000 });

    // Repoint to the wrong repository. The dialog says what this costs BEFORE save.
    await page.getByRole("button", { name: /^edit$/i }).click();
    await page.getByLabel(/^repository$/i).fill(MISSING_REPO);
    await expect(page.getByTestId("source-warning")).toBeVisible();
    await page.getByRole("button", { name: /^save$/i }).click();

    // Server truth: bundle discarded, row withdrawn. Not an error — an honest state.
    await expect(page.getByText("No bundle")).toBeVisible({ timeout: 30_000 });
    await expect(page.getByText("Published")).toHaveCount(0);

    // Correct the typo back, refetch, republish.
    await page.getByRole("button", { name: /^edit$/i }).click();
    await page.getByLabel(/^repository$/i).fill(SAMPLE_REPO);
    await page.getByRole("button", { name: /^save$/i }).click();
    await expect(page.getByLabel(/^repository$/i)).toHaveCount(0, { timeout: 30_000 });

    await page.getByRole("button", { name: /fetch bundle/i }).click();
    await page.getByRole("button", { name: /^create fleet library$/i }).click();
    await expect(page.getByText("Draft")).toBeVisible({ timeout: IMPORT_TIMEOUT });

    await page.getByRole("button", { name: /^publish$/i }).click();
    await expect(page.getByText("Published")).toBeVisible({ timeout: 30_000 });
  });
});

