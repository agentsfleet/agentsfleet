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
import { SAMPLE_LIBRARY_REPO } from "@/lib/fleet-library-source";
import { signInAs } from "./fixtures/auth";
import { skillMd, triggerMd } from "./fixtures/seed";
import { workspaceUrlPattern } from "./fixtures/nav";
import { FIXTURE_KEY } from "./fixtures/constants";

const ADMIN_PATH = "/admin/fleet-libraries";
const NAV_LABEL = "Fleet library";

// Published at agentsfleet/<id>. The catalog id is the bundle's SKILL.md
// frontmatter name — not the repository path the operator types.
const SAMPLE_ENTRY_ID = SAMPLE_LIBRARY_REPO.slice(SAMPLE_LIBRARY_REPO.indexOf("/") + 1);

// A repository that does not exist, to drive the importer's fetch failure into the
// dialog rather than a crash or a silent close.
const MISSING_REPO = "agentsfleet/definitely-not-a-fleet-bundle";

// The catalog id an uploaded bundle lands under — its SKILL.md frontmatter name.
// Owned by this suite so a leftover from an interrupted run can be swept.
const UPLOADED_ENTRY_ID = "acceptance-uploaded-bundle";

// How long this suite waits on an import, and why it is longer than the budget
// the dashboard itself gives one. `ONBOARD_BUNDLE_TIMEOUT_MS` in
// `lib/api/fleet-library.ts` is 30s; waiting past that means a real timeout
// reaches the operator as the dialog's own message, which is what this suite
// reads, rather than as this expectation expiring first and reporting a dialog
// that never closed. At 60s the wait was longer than the budget in wall-clock
// terms but the dialog stays open on error forever, so the run still burned the
// full minute before saying anything useful — see `expectImportSucceeded`.
const IMPORT_TIMEOUT = 45_000;
// How often `expectImportSucceeded` re-reads the dialog. Short enough that a
// failure is reported in about the time it took to fail, long enough that the
// poll is not itself load on a remote environment.
const IMPORT_POLL_INTERVAL_MS = 250;
// What the backend answers when the bundle could not be fetched from its
// source: `Error::code()` in `rustd/crates/afd_library/src/error.rs` maps every
// `Source`, `Github`, `Archive` and `Redirect` failure to it. The dialog prints
// the code, which is what lets this suite tell a third party's bad day from a
// defect of ours.
const UPSTREAM_FETCH_FAILED_CODE = "UZ-BUNDLE-004";
const CLERK_TOKEN_EXPIRY_PROOF_MS = 70_000;
// The longest walk in the suite: a GitHub import plus four identity switches
// plus publish/unpublish round-trips, every leg a remote round-trip. Five
// minutes, the ceiling it has always carried, now stated rather than multiplied
// off the import budget: the job itself stops at 13 minutes
// (`.github/workflows/deploy-dev-acceptance.yml`), so a walk that tracked a
// raised import budget as a multiple would outlive the job that runs it and
// lose the per-test report in a job-level kill.
const PLATFORM_JOURNEY_TIMEOUT = 300_000;

async function gotoRejectedAdminPath(page: Page): Promise<void> {
  await page.goto(ADMIN_PATH).catch((error: unknown) => {
    if (!(error instanceof Error) || !error.message.includes("ERR_ABORTED")) throw error;
  });
  // An aborted navigation leaves the page on about:blank, which reports an
  // empty URL — still proof the admin surface was refused. Assert on the
  // settled URL so the empty case reads as the rejection it is.
  await expect
    .poll(() => page.url(), { timeout: 10_000 })
    .not.toContain(ADMIN_PATH);
}

// Establish the sample as an unpublished catalog entry. Re-adding the same
// repository is the refetch path, so a rerun keeps any installed workspace copy
// while restoring the draft visibility this suite starts from.
async function addSampleFleet(page: Page) {
  await page.goto(ADMIN_PATH);
  // Self-heal: an interrupted earlier run can leave the entry pointing at a
  // deliberately-broken repository. Restore it before anything else, or the
  // re-add below refetches from the poisoned source and never reaches Draft.
  const editButton = sampleRow(page).getByRole("button", { name: /^edit$/i });
  if (await editButton.isVisible()) {
    await editButton.click();
    const repoField = page.getByLabel(/^repository$/i);
    if ((await repoField.inputValue()) !== SAMPLE_LIBRARY_REPO) {
      await repoField.fill(SAMPLE_LIBRARY_REPO);
      await page.getByRole("button", { name: /^save$/i }).click();
      await expect(repoField).toHaveCount(0, { timeout: 30_000 });
      await page.reload();
    } else {
      await page.getByRole("button", { name: /^cancel$/i }).click();
    }
  }
  const unpublish = sampleRow(page).getByRole("button", { name: /^unpublish$/i });
  if (await unpublish.isVisible()) {
    await unpublish.click();
    await expect(sampleRow(page).getByText("Draft")).toBeVisible({ timeout: 30_000 });
  }
  await page.getByRole("button", { name: /create fleet library/i }).click();
  await page.getByLabel(/repository/i).fill(SAMPLE_LIBRARY_REPO);
  await submitCreate(page);
  await expectImportSucceeded(page);
  await expect(sampleRow(page)).toBeVisible({ timeout: IMPORT_TIMEOUT });
  await expect(sampleRow(page).getByText("Draft")).toBeVisible();
}

// The dialog's submit; the page's own "Create fleet library" trigger sits behind
// the modal overlay and is not it.
async function submitCreate(page: Page) {
  await page.getByRole("dialog").getByRole("button", { name: /^create$/i }).click();
}

// Waits for the import dialog to close, and reacts the moment it instead shows
// its error — with that error's own words.
//
// Waiting only for the dialog to disappear is a blind wait: the dialog stays
// open on failure, so a failed import burned the whole IMPORT_TIMEOUT and then
// reported "expected 0, got 1", naming nothing. The run that sent us here spent
// 64s to say that, when the dialog had been showing "The request timed out /
// The backend took too long to answer" since second ten.
//
// The two failures are not the same kind of news, so they do not get the same
// verdict. GitHub is a third party this suite does not control: it rate-limits,
// it 5xxs, it goes down, and none of that is a defect in the dashboard. The
// backend already draws that line — `Error::code()` in afd_library answers
// UZ-BUNDLE-004 for every `Source`, `Github`, `Archive` and `Redirect` failure —
// and the dialog prints the code, so the suite can read it rather than guess
// from prose. That case SKIPS: the run moves on and says why. Everything else,
// our own transport timeout above all, is ours and fails.
async function expectImportSucceeded(page: Page) {
  const dialog = page.getByRole("dialog");
  // Scoped to the dialog: the page has its own toast `alert` region, and the
  // one that means "this import failed" is the one inside the modal.
  const failure = dialog.getByRole("alert");
  const deadline = Date.now() + IMPORT_TIMEOUT;
  while (Date.now() < deadline) {
    if ((await dialog.count()) === 0) return;
    if (await failure.isVisible().catch(() => false)) {
      const reason = (await failure.innerText()).replace(/\s+/g, " ").trim();
      test.skip(
        reason.includes(UPSTREAM_FETCH_FAILED_CODE),
        `GitHub could not serve the bundle (${UPSTREAM_FETCH_FAILED_CODE}); not a dashboard defect: ${reason}`,
      );
      throw new Error(`the import dialog reported a failure instead of closing: ${reason}`);
    }
    await page.waitForTimeout(IMPORT_POLL_INTERVAL_MS);
  }
  throw new Error(`the import dialog neither closed nor reported a failure within ${IMPORT_TIMEOUT}ms`);
}

// Removes the uploaded entry if it is there. Used both to self-heal an
// interrupted run and to clean up after this suite's own upload.
async function removeUploadedFleet(page: Page) {
  await expect(page.getByRole("button", { name: /create fleet library/i })).toBeVisible();
  const row = uploadedRow(page);
  if (!(await row.getByRole("button", { name: /^delete$/i }).isVisible())) return;
  await row.getByRole("button", { name: /^delete$/i }).click();
  // ConfirmDialog is an `alertdialog`, not a `dialog` — getByRole("dialog")
  // does not match it, so the confirm must be addressed by its real role.
  await page.getByRole("alertdialog").getByRole("button", { name: /^delete$/i }).click();
  await expect(uploadedRow(page)).toHaveCount(0, { timeout: 30_000 });
}

function uploadedRow(page: Page) {
  return page.getByRole("row", { name: new RegExp(`Copy fleet id: ${UPLOADED_ENTRY_ID}`) });
}

function sampleRow(page: Page) {
  return page.getByRole("row", { name: new RegExp(`Copy fleet id: ${SAMPLE_ENTRY_ID}`) });
}

function galleryCards(page: Page) {
  return page.getByTestId(`library-card-${SAMPLE_ENTRY_ID}`);
}

// The gallery is one merged page, newest first, and a person reaches the rest
// through "Load more". A published platform entry keeps the `created_at` it
// had as a draft, so in a workspace with uploads of its own it can sit pages
// deep — and whether a card is THERE is a question about the whole gallery,
// reached the way a person reaches it. Bounded in clicks, never in waiting:
// a gallery still offering more past the bound is its own failure.
const LOAD_MORE_LABEL = /^load more$/i;
const GALLERY_SECTION_LABEL = "Fleet library";
const GALLERY_PAGE_BOUND = 20;

async function loadWholeGallery(page: Page) {
  await expect(page.getByText(GALLERY_SECTION_LABEL, { exact: true })).toBeVisible();
  const loadMore = page.getByRole("button", { name: LOAD_MORE_LABEL });
  for (let pages = 0; pages < GALLERY_PAGE_BOUND; pages += 1) {
    if (!(await loadMore.isVisible())) return;
    const shown = await page.getByRole("article").count();
    await loadMore.click();
    await expect(page.getByRole("article")).not.toHaveCount(shown);
  }
  throw new Error(`the gallery still offered more after ${GALLERY_PAGE_BOUND} pages`);
}

test.describe("platform fleet catalog", () => {
  test.describe.configure({ timeout: PLATFORM_JOURNEY_TIMEOUT });

  test("a workspace user never sees the operator surface", async ({ page }) => {
    await signInAs(page, FIXTURE_KEY.regular);
    await page.goto("/");

    await expect(page.getByRole("link", { name: NAV_LABEL })).toHaveCount(0);

    // Even by direct URL: the page redirects rather than rendering an action the
    // session could not take.
    await gotoRejectedAdminPath(page);
  });

  test("an operator reaches the surface from the nav", async ({ page }) => {
    await signInAs(page, FIXTURE_KEY.operator);
    await page.goto("/");

    await page.getByRole("link", { name: NAV_LABEL }).click();

    await expect(page).toHaveURL(new RegExp(ADMIN_PATH));
    await expect(page.getByRole("heading", { level: 1, name: NAV_LABEL })).toBeVisible();
  });

  test("a repository that cannot be imported keeps the dialog open with the error", async ({
    page,
  }) => {
    await signInAs(page, FIXTURE_KEY.operator);
    await page.goto(ADMIN_PATH);

    await page.getByRole("button", { name: /create fleet library/i }).click();
    await page.getByLabel(/repository/i).fill(MISSING_REPO);
    await submitCreate(page);

    // The dialog stays mounted with the failure shown — the operator corrects the
    // repository in place rather than losing what they typed.
    await expect(page.getByRole("alert")).toBeVisible({ timeout: 30_000 });
    await expect(page.getByLabel(/repository/i)).toBeVisible();
  });

  // GitHub is not the only source the importer takes — `resolve.resolve` has always
  // routed `upload` on the platform tier too. Until now only the operator's screen
  // was GitHub-only, so a bundle that lived on a laptop had to be pushed to a
  // repository first just to be reviewed.
  test("an operator creates a fleet library from a bundle on their machine", async ({ page }) => {
    await signInAs(page, FIXTURE_KEY.operator);
    await page.goto(ADMIN_PATH);
    await removeUploadedFleet(page);

    await page.getByRole("button", { name: /create fleet library/i }).click();
    await page.getByRole("tab", { name: /upload from computer/i }).click();
    await page.getByLabel("SKILL.md").fill(skillMd(UPLOADED_ENTRY_ID));
    await page.getByLabel("TRIGGER.md").fill(triggerMd(UPLOADED_ENTRY_ID));
    await submitCreate(page);

    await expectImportSucceeded(page);
    const row = uploadedRow(page);
    await expect(row).toBeVisible({ timeout: IMPORT_TIMEOUT });
    // Same publish gate as every other source: an upload is not a shortcut into
    // a tenant's gallery.
    await expect(row.getByText("Draft")).toBeVisible();
    // Pasted bytes came from no revision, so the row advertises no repository to
    // click through to.
    await expect(row.getByRole("link", { name: /open on github/i })).toHaveCount(0);
    // And no Fetch either — there is no revision to re-read. The affordance is
    // absent rather than disabled, so it cannot open a dialog with nothing in it.
    await expect(row.getByRole("button", { name: /^fetch/i })).toHaveCount(0);

    await removeUploadedFleet(page);
  });

  // The heart of the milestone. A fleet an operator has added is NOT live: the
  // publish gate is what protects every tenant from an unreviewed bundle, and it is
  // worthless unless a draft is genuinely unreachable.
  test("an added fleet is a draft no workspace can see until it is published", async ({ page }) => {
    await signInAs(page, FIXTURE_KEY.operator);
    await addSampleFleet(page);

    // It exists, and it is a draft. The table says so.
    await expect(sampleRow(page).getByText("Draft")).toBeVisible();

    // A plain workspace user cannot see it. Not hidden-but-installable — absent.
    await signInAs(page, FIXTURE_KEY.regular);
    await page.goto("/");
    // The root redirect lands on the workspace wall.
    await expect(page).toHaveURL(workspaceUrlPattern("fleets"));
    const workspacePath = new URL(page.url()).pathname.match(/^\/w\/[^/]+/)![0];
    await page.goto(`${workspacePath}/fleets/new`);
    await loadWholeGallery(page);
    await expect(galleryCards(page)).toHaveCount(0);

    // The operator publishes. This is the only act that opens the door.
    await signInAs(page, FIXTURE_KEY.operator);
    await page.goto(ADMIN_PATH);
    await sampleRow(page).getByRole("button", { name: /^publish$/i }).click();
    await expect(sampleRow(page).getByText("Published")).toBeVisible({ timeout: 30_000 });

    // Now the same workspace user can install it — exactly once. The gallery is
    // where a duplicate catalog row would show, so this also pins that re-adding
    // upserts rather than minting a second entry.
    await signInAs(page, FIXTURE_KEY.regular);
    await page.goto(`${workspacePath}/fleets/new`);
    await loadWholeGallery(page);
    await expect(galleryCards(page).first()).toBeVisible({ timeout: 30_000 });
    await expect(galleryCards(page)).toHaveCount(1);

    // And withdrawing takes it back out. Unpublish is a real withdrawal, not a
    // cosmetic flag: the fleet leaves the gallery it was installable from.
    await signInAs(page, FIXTURE_KEY.operator);
    await page.goto(ADMIN_PATH);
    await sampleRow(page).getByRole("button", { name: /^unpublish$/i }).click();
    await expect(sampleRow(page).getByText("Draft")).toBeVisible({ timeout: 30_000 });

    await signInAs(page, FIXTURE_KEY.regular);
    await page.goto(`${workspacePath}/fleets/new`);
    await loadWholeGallery(page);
    await expect(galleryCards(page)).toHaveCount(0);
  });

  // The pencil: the install-gate copy is the platform's voice, and the operator owns
  // it. A bundle refetch must never undo what they wrote.
  test("the operator's install-gate copy survives a bundle refetch", async ({ page }) => {
    // Real install-gate copy, deliberately distinct from the bundle's own
    // frontmatter description so the survives-refetch assertion can tell the
    // operator's voice from the bundle's. This is what dev's catalog shows.
    const COPY = "Reviews pull requests and posts focused review comments.";

    await signInAs(page, FIXTURE_KEY.operator);
    await addSampleFleet(page);

    // Cross Clerk's original token lifetime while the dashboard stays open.
    // Saving afterward is a real Server Action proof that the product keeper,
    // not an acceptance-only token call, preserved the signed-in session.
    await page.waitForTimeout(CLERK_TOKEN_EXPIRY_PROOF_MS);

    await sampleRow(page).getByRole("button", { name: /^edit$/i }).click();
    await page.getByLabel(/^description$/i).fill(COPY);
    await page.getByRole("button", { name: /^save$/i }).click();
    await expect(page.getByLabel(/^description$/i)).toHaveCount(0, { timeout: 30_000 });

    // Re-fetch the bundle from the same repository — the update path.
    await sampleRow(page).getByRole("button", { name: /fetch update/i }).click();
    await page.getByRole("dialog").getByRole("button", { name: /^fetch update$/i }).click();
    await expect(sampleRow(page)).toBeVisible({ timeout: IMPORT_TIMEOUT });

    // The operator's copy is still there. The server keeps `description` out of the
    // refetch upsert precisely so this holds (M128 Invariant 4).
    await sampleRow(page).getByRole("button", { name: /^edit$/i }).click();
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
    await sampleRow(page).getByRole("button", { name: /^publish$/i }).click();
    await expect(sampleRow(page).getByText("Published")).toBeVisible({ timeout: 30_000 });

    // Repoint to the wrong repository. The dialog says what this costs BEFORE save.
    await sampleRow(page).getByRole("button", { name: /^edit$/i }).click();
    await page.getByLabel(/^repository$/i).fill(MISSING_REPO);
    await expect(page.getByTestId("source-warning")).toBeVisible();
    await page.getByRole("button", { name: /^save$/i }).click();

    // Server truth: bundle discarded, row withdrawn. Not an error — an honest state.
    await expect(sampleRow(page).getByText("No bundle")).toBeVisible({ timeout: 30_000 });
    await expect(sampleRow(page).getByText("Published")).toHaveCount(0);

    // Correct the typo back, refetch, republish.
    await sampleRow(page).getByRole("button", { name: /^edit$/i }).click();
    await page.getByLabel(/^repository$/i).fill(SAMPLE_LIBRARY_REPO);
    await page.getByRole("button", { name: /^save$/i }).click();
    await expect(page.getByLabel(/^repository$/i)).toHaveCount(0, { timeout: 30_000 });

    // The refetch dialog's repository is read-only and prefills from the
    // row; reload so the prefill reflects the corrective save above.
    await page.reload();
    await sampleRow(page).getByRole("button", { name: /fetch bundle/i }).click();
    await page.getByRole("dialog").getByRole("button", { name: /^fetch update$/i }).click();
    await expect(sampleRow(page).getByText("Draft")).toBeVisible({ timeout: IMPORT_TIMEOUT });

    await sampleRow(page).getByRole("button", { name: /^publish$/i }).click();
    await expect(sampleRow(page).getByText("Published")).toBeVisible({ timeout: 30_000 });
  });
});
