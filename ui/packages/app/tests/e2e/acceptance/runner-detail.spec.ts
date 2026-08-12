/**
 * runner-detail.spec.ts — the operator's runner triage walk, end to end.
 *
 * Wire: seed a fleet whose SKILL.md carries valid frontmatter and an EMPTY
 * body → steer it once from the console → the runner leases the delivery and
 * fails closed before any model call (child_exec's no-instructions branch
 * reports a startup-posture failure) → the operator opens /admin/runners,
 * clicks the host's card, lands on that runner's Leases, reads the failure
 * as the shared plain-English sentence, and opens Review lease from the row.
 *
 * The empty-body failure is the one deterministic, model-free way to place a
 * failed lease on a runner from the outside: the self-plane lease protocol is
 * runner-token-only, and a healthy seeded fleet would settle succeeded. The
 * arrangement half runs as the regular tenant; the walk half re-signs-in as
 * the operator fixture — the only identity whose scopes open /admin surfaces.
 */
import { expect, test, type Locator, type Page } from "@playwright/test";
import { LEASE_OUTCOME, type RunnerLeaseResponse, type RunnerListResponse } from "@/lib/api/runners";
import {
  CLEAR_WORKSPACE_FILTER_LABEL,
  LEASES_EMPTY_TITLE,
  LEASES_TABLE_LABEL,
  WORKSPACE_FILTER_PARAM,
  WORKSPACE_LABEL,
} from "@/app/(dashboard)/admin/runners/[runnerId]/components/runner-copy";
import { failureSentenceFor } from "@/lib/events/event-summary";
import { SOURCE_KIND_UPLOAD } from "@/lib/types";
import { clientFor } from "./fixtures/api-client";
import { signInAs } from "./fixtures/auth";
import { FIXTURE_KEY } from "./fixtures/constants";
import {
  emptyBodySkillMd,
  getDefaultWorkspaceId,
  triggerMd,
  waitForFleetActive,
} from "./fixtures/seed";
import { cleanWorkspaceFleets } from "./fixtures/teardown";
import { workspaceHref, workspaceUrlPattern } from "./fixtures/nav";

const FLEET_NAME_PREFIX = "runner-detail-";
const RENDER_TIMEOUT_MS = 15_000;
// The failure must ride delivery → lease → child start → terminal report →
// event settle. No model round-trip is involved, but the pipeline crosses the
// queue and the runner heartbeat cadence, so this leg gets its own budget.
const LEASE_SETTLE_TIMEOUT_MS = 120_000;
const LEASE_POLL_INTERVAL_MS = 2_000;
const FLEET_RUNNERS_PATH = "/v1/fleets/runners";
// The no-instructions branch reports this class; the UI must render the
// shared sentence for it and never the tag itself.
const EXPECTED_FAILURE_TAG = "startup_posture";

interface OnboardTemplateResp {
  id: string;
}

interface CreateFleetResp {
  fleet_id: string;
  name: string;
}

interface FailedLeaseLocation {
  runnerId: string;
  hostId: string;
}

// The lease read is per-runner, and which enrolled host takes the delivery is
// the scheduler's call — so the poll walks every runner's first lease page
// until the fleet's failed lease surfaces somewhere.
async function findFailedLease(fleetId: string): Promise<FailedLeaseLocation | null> {
  const operator = clientFor(FIXTURE_KEY.operator);
  const runners = await operator.get<RunnerListResponse>(FLEET_RUNNERS_PATH);
  for (const runner of runners.items) {
    const leases = await operator.get<RunnerLeaseResponse>(
      `${FLEET_RUNNERS_PATH}/${runner.id}/leases?limit=50`,
    );
    const failed = leases.items.find(
      (lease) => lease.fleet_id === fleetId && lease.outcome === LEASE_OUTCOME.failed,
    );
    if (failed) return { runnerId: runner.id, hostId: runner.host_id };
  }
  return null;
}

// Well-formed UUIDv7 that no workspace owns: the daemon refuses a MALFORMED
// filter with a 400, but an unknown one must simply match nothing — an empty
// page, never an error state (Dimension 1.2's other half, seen from the UI).
const UNOWNED_WORKSPACE_ID = "01890a5d-ac96-774b-bcce-b302099a8057";

// Mirror of the workspace cell's own truncation. Importing the constant would
// drag a "use client" module — React and next/navigation with it — into the
// Playwright process; the sibling unit test pins the two against each other.
const WORKSPACE_ID_DISPLAY_CHARS = 8;
function shortWorkspaceId(workspaceId: string): string {
  return workspaceId.length > WORKSPACE_ID_DISPLAY_CHARS
    ? `${workspaceId.slice(0, WORKSPACE_ID_DISPLAY_CHARS)}…`
    : workspaceId;
}

// Each workspace cell carries its full id in `title`. A filtered page whose
// rows disagree with the filter is exactly the bug this dimension exists to
// catch — the chip rendering is not, on its own, proof the READ narrowed.
async function expectEveryRowInWorkspace(table: Locator, workspaceId: string): Promise<void> {
  const workspaceLinks = table.getByRole("link");
  const count = await workspaceLinks.count();
  expect(count).toBeGreaterThan(0);
  for (let index = 0; index < count; index += 1) {
    await expect(workspaceLinks.nth(index)).toHaveAttribute("title", workspaceId);
  }
}

interface SeededFailedLease extends FailedLeaseLocation {
  workspaceId: string;
  fleetName: string;
}

// Arrange, as the regular tenant: a delivery that fails at startup, settled
// onto whichever runner the scheduler picked. Both walks below start here —
// each needs one failed lease, on a known runner, in a known workspace.
async function seedFailedLease(page: Page): Promise<SeededFailedLease> {
  const ws = await getDefaultWorkspaceId(FIXTURE_KEY.regular);
  const tag = Math.random().toString(36).slice(2, 8);
  const name = `${FLEET_NAME_PREFIX}${tag}`;
  const tenant = clientFor(FIXTURE_KEY.regular);
  const library = await tenant.post<OnboardTemplateResp>(
    `/v1/workspaces/${ws}/fleet-libraries`,
    {
      source_kind: SOURCE_KIND_UPLOAD,
      skill_markdown: emptyBodySkillMd(name),
      trigger_markdown: triggerMd(name),
    },
  );
  const fleet = await tenant.post<CreateFleetResp>(`/v1/workspaces/${ws}/fleets`, {
    tenant_library_id: library.id,
    name,
  });
  await waitForFleetActive(FIXTURE_KEY.regular, ws, fleet.fleet_id);

  await signInAs(page, FIXTURE_KEY.regular);
  await page.goto(workspaceHref(ws, `fleets/${fleet.fleet_id}`));
  await expect(page).toHaveURL(workspaceUrlPattern(`fleets/${fleet.fleet_id}`));

  const composer = page.getByLabel("Chat composer");
  await expect(composer).toBeVisible({ timeout: RENDER_TIMEOUT_MS });
  await composer.getByPlaceholder(/message this fleet/i).fill(`fail-${tag}`);
  await composer.getByRole("button", { name: /send/i }).click();

  // The settle proves the new operator-plane lease read end-to-end: the
  // failed lease must surface through GET /v1/fleets/runners/{id}/leases.
  let location: FailedLeaseLocation | null = null;
  await expect
    .poll(
      async () => {
        location = await findFailedLease(fleet.fleet_id);
        return location !== null;
      },
      { timeout: LEASE_SETTLE_TIMEOUT_MS, intervals: [LEASE_POLL_INTERVAL_MS] },
    )
    .toBe(true);
  return { ...location!, workspaceId: ws, fleetName: name };
}

test.describe("runner detail", () => {
  test("wall → detail → failed lease sentence → Review lease", async ({ page }) => {
    test.setTimeout(LEASE_SETTLE_TIMEOUT_MS + 120_000);

    const { runnerId, hostId, fleetName: name } = await seedFailedLease(page);

    // ── Walk: the operator's triage path ──
    await signInAs(page, FIXTURE_KEY.operator);
    await page.goto("/admin/runners");
    const card = page.getByRole("link", {
      name: new RegExp(`^Inspect runner: ${hostId}`),
    });
    await expect(card).toBeVisible({ timeout: RENDER_TIMEOUT_MS });
    await card.click();

    // The whole card links to the addressable detail page, landing on Leases.
    await expect(page).toHaveURL(new RegExp(`/admin/runners/${runnerId}$`), {
      timeout: RENDER_TIMEOUT_MS,
    });
    const leases = page.getByRole("table", { name: "Runner leases" });
    await expect(leases).toBeVisible({ timeout: RENDER_TIMEOUT_MS });

    // The failed row reads the shared sentence; the machine tag never renders.
    const failedRow = leases
      .getByRole("row")
      .filter({ hasText: name })
      .filter({ hasText: failureSentenceFor(EXPECTED_FAILURE_TAG) })
      .first();
    await expect(failedRow).toBeVisible({ timeout: RENDER_TIMEOUT_MS });
    await expect(page.getByText(EXPECTED_FAILURE_TAG)).toHaveCount(0);

    // Activating the row opens Review lease with the lease's facts.
    await failedRow.click();
    const review = page.getByRole("dialog", { name: "Review lease" });
    await expect(review).toBeVisible({ timeout: RENDER_TIMEOUT_MS });
    await expect(review.getByText("Fencing token")).toBeVisible();
    await expect(review.getByText(failureSentenceFor(EXPECTED_FAILURE_TAG))).toBeVisible();
    await expect(review.getByText(/request_json|request payload/i)).toHaveCount(0);
  });

  test("test_lease_table_workspace_filter_deep_link: the filter is addressable state", async ({
    page,
  }) => {
    test.setTimeout(LEASE_SETTLE_TIMEOUT_MS + 120_000);

    const { runnerId, workspaceId, fleetName } = await seedFailedLease(page);
    await signInAs(page, FIXTURE_KEY.operator);

    const detailPath = `/admin/runners/${runnerId}`;
    const leases = page.getByRole("table", { name: LEASES_TABLE_LABEL });
    const seededRow = leases.getByRole("row").filter({ hasText: fleetName });
    const chip = page.getByText(`${WORKSPACE_LABEL} ${shortWorkspaceId(workspaceId)}`);

    // ── Deep link: the URL alone puts the table in the filtered state ──
    await page.goto(`${detailPath}?${WORKSPACE_FILTER_PARAM}=${workspaceId}`);
    await expect(leases).toBeVisible({ timeout: RENDER_TIMEOUT_MS });
    await expect(chip).toBeVisible();
    await expect(seededRow).toHaveCount(1);
    await expectEveryRowInWorkspace(leases, workspaceId);

    // ── Reload: the filter lives in the URL, not in component state ──
    await page.reload();
    await expect(leases).toBeVisible({ timeout: RENDER_TIMEOUT_MS });
    await expect(page).toHaveURL(
      new RegExp(`[?&]${WORKSPACE_FILTER_PARAM}=${workspaceId}(&|$)`),
    );
    await expect(chip).toBeVisible();
    await expect(seededRow).toHaveCount(1);

    // ── Clearing is a navigation, so Back returns to the filtered feed ──
    await page.getByRole("button", { name: CLEAR_WORKSPACE_FILTER_LABEL }).click();
    await expect(page).toHaveURL(new RegExp(`${detailPath}$`), { timeout: RENDER_TIMEOUT_MS });
    await expect(chip).toBeHidden();
    await page.goBack();
    await expect(chip).toBeVisible({ timeout: RENDER_TIMEOUT_MS });

    // ── Negative: a well-formed id nobody owns is an empty page, not an error ──
    await page.goto(`${detailPath}?${WORKSPACE_FILTER_PARAM}=${UNOWNED_WORKSPACE_ID}`);
    await expect(page.getByText(LEASES_EMPTY_TITLE)).toBeVisible({ timeout: RENDER_TIMEOUT_MS });
    await expect(seededRow).toHaveCount(0);
  });

  test.afterEach(async () => {
    const ws = await getDefaultWorkspaceId(FIXTURE_KEY.regular);
    await cleanWorkspaceFleets(FIXTURE_KEY.regular, ws, FLEET_NAME_PREFIX);
  });
});
