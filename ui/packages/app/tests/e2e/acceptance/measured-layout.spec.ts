/**
 * measured-layout.spec.ts — layout outcomes only a real browser computes.
 *
 * The unit suite renders in happy-dom, which lays nothing out: a dialog that
 * cannot scroll, a grid that wraps, or a clamp that does not clip all pass a
 * class-name check there. These tests measure the rendered page instead.
 *
 * Each dialog is opened in a window too short for its form. Radix locks the
 * page scroll while a dialog is open, so the footer is reachable only if the
 * dialog itself is bounded to the viewport and scrolls.
 *
 * A paginating table fills the page it sits on only when two halves agree: the
 * page gives it a definite height and the table claims it. Events once shipped
 * a min-height shell that left its table short, so Secrets and Events are
 * measured against their page. The platform Fleet library table ends at its
 * rows by design, so it is measured for that instead: no empty band inside the
 * table, in a window that holds a full page of rows.
 */
import { expect, type Locator, type Page, test } from "@playwright/test";
import { v7 } from "uuid";
import { signInAs } from "./fixtures/auth";
import { FIXTURE_KEY, LIBRARY_LAYOUT_WORKSPACE_NAME } from "./fixtures/constants";
import { clientFor } from "./fixtures/api-client";
import { ensureSecondWorkspace, getDefaultWorkspaceId, seedFleet, waitForFleetActive } from "./fixtures/seed";
import { cleanWorkspaceFleets } from "./fixtures/teardown";
import { fixtureDefinition } from "./fixtures/definition";
import { publishFixtureLibrary, withStagedPlatformEntry } from "./fixtures/native-setup";
import { workspaceHref } from "./fixtures/nav";
import { EDIT, EDIT_TITLE } from "@/app/(dashboard)/admin/fleet-libraries/library-copy";
import { OPEN_RUNNER_LABEL } from "@/app/(dashboard)/admin/runners/[runnerId]/components/runner-copy";
import { EDIT_POLICY_LABEL } from "@/app/(dashboard)/admin/runners/components/RunnerListCells";

const SHORT_WINDOW = { width: 1280, height: 480 };
const LIBRARY_SHORT_WINDOW = { ...SHORT_WINDOW, height: 320 };
const WIDE_WINDOW = { width: 1280, height: 900 };
const PHONE_WINDOW = { width: 390, height: 844 };
const RENDER_TIMEOUT_MS = 15_000;
const CREATE_RUNNER = "Create runner";
const EDIT_POLICY = "Edit runner";
// Sub-pixel rounding between a line box and its element's height.
const LINE_TOLERANCE_PX = 1;
const TALL_WINDOW = { width: 1280, height: 1400 };
// A full page of 25 platform library rows is about 1,310 px, so this one leaves room
// below it; with no room, a stretched table and a fitted one measure the same.
const LIBRARY_WINDOW = { width: 1280, height: 2000 };
const FULL_HEIGHT_PAGE = '[data-page-layout="full-height"]';
const DATA_TABLE = '[data-testid="data-table"]';
// The scrolling box around the rows; its height is what a stretched table grows.
const TABLE_VIEWPORT = `${DATA_TABLE} > [role="region"]`;
// Section padding between the table's frame and the page's bottom edge. A
// table that collapsed to its rows stops hundreds of pixels short in this window.
const FILL_GAP_PX = 48;
// The workspace Library page the inspection test opens.
const WORKSPACE_LIBRARY = "library";
// Each test seeds the library entry it reads: on a fresh deployment no other
// project is guaranteed to have created one before this suite runs.
const PLATFORM_ENTRY_ID = "e2e-measured-layout";
const GALLERY_TEMPLATE = "e2e-measured-gallery";
// Events lists a row only once a fleet has admitted one: one steered fleet gives it.
const TABLE_FILL_FLEET_PREFIX = "e2e-table-fill-";
const TABLE_FILL_MESSAGE = "measure the events table";

async function expectTableFillsPage(page: Page, path: string): Promise<void> {
  await page.goto(path);
  const layout = page.locator(FULL_HEIGHT_PAGE);
  await expect(layout.locator(DATA_TABLE).first()).toBeVisible({ timeout: RENDER_TIMEOUT_MS });
  const measured = await layout.evaluate((element, tableSelector) => {
    const pageBox = element.getBoundingClientRect();
    const tableBox = element.querySelector(tableSelector)!.getBoundingClientRect();
    const contentBottom = pageBox.bottom - Number.parseFloat(getComputedStyle(element).paddingBottom);
    const canvasBottom = element.closest("main")!.getBoundingClientRect().bottom;
    return { pastCanvas: pageBox.bottom - canvasBottom, gap: contentBottom - tableBox.bottom };
  }, DATA_TABLE);
  // A page that grows with its rows has no definite height to give the table.
  expect(measured.pastCanvas, `${path} grows past the canvas`).toBeLessThanOrEqual(LINE_TOLERANCE_PX);
  expect(measured.gap, `${path} leaves its table short of the page`).toBeLessThanOrEqual(FILL_GAP_PX);
}

// The rows' own height, not `scrollHeight`: `scrollHeight` never drops below
// `clientHeight`, so it cannot see an empty band under the last row.
async function expectTableEndsAtRows(page: Page, path: string): Promise<void> {
  await page.goto(path);
  const layout = page.locator(FULL_HEIGHT_PAGE);
  await expect(layout.locator(DATA_TABLE).first()).toBeVisible({ timeout: RENDER_TIMEOUT_MS });
  const measured = await layout.evaluate((element, viewportSelector) => {
    const viewport = element.querySelector(viewportSelector);
    const rows = viewport?.querySelector("table");
    if (!viewport || !rows) throw new Error(`no table inside ${viewportSelector}`);
    return {
      overflow: viewport.scrollHeight - viewport.clientHeight,
      slack: viewport.clientHeight - rows.getBoundingClientRect().height,
    };
  }, TABLE_VIEWPORT);
  expect(measured.overflow, `${path} has more rows than the window holds, so a stretch would not show`).toBeLessThanOrEqual(
    LINE_TOLERANCE_PX,
  );
  expect(measured.slack, `${path} stretches its table past its rows`).toBeLessThanOrEqual(LINE_TOLERANCE_PX);
}

// The workspace no other spec clears library entries from.
async function layoutWorkspaceId(): Promise<string> {
  return (await ensureSecondWorkspace(FIXTURE_KEY.regular, LIBRARY_LAYOUT_WORKSPACE_NAME)).id;
}

// Onboards `name` into `workspaceId`'s own Fleet library.
async function seedLibraryEntry(workspaceId: string, name: string): Promise<void> {
  await publishFixtureLibrary(FIXTURE_KEY.regular, workspaceId, fixtureDefinition(name));
}

// Onboards one platform row for the operator's library page, and removes it after.
async function withPlatformEntry(body: (name: string) => Promise<void>): Promise<void> {
  const name = `${PLATFORM_ENTRY_ID}-${crypto.randomUUID()}`;
  await withStagedPlatformEntry(await getDefaultWorkspaceId(FIXTURE_KEY.operator), name, () => body(name));
}

async function expectFooterReachable(dialog: Locator, footerButton: string): Promise<void> {
  await expect(dialog).toBeVisible({ timeout: RENDER_TIMEOUT_MS });
  // The form must overflow this window, or the reach below proves nothing.
  await expect.poll(() => dialog.evaluate((element) => element.scrollHeight > element.clientHeight)).toBe(true);
  const footer = dialog.getByRole("button", { name: footerButton, exact: true });
  await footer.scrollIntoViewIfNeeded();
  await expect(footer).toBeInViewport();
}

async function openFirstRunnerPolicy(page: Page): Promise<Locator> {
  await signInAs(page, FIXTURE_KEY.policyEditor);
  await page.goto("/admin/runners");
  const runner = page.getByRole("link", { name: new RegExp(`^${OPEN_RUNNER_LABEL}`) }).first();
  await expect(runner).toBeVisible({ timeout: RENDER_TIMEOUT_MS });
  await runner.click();
  await page.getByRole("button", { name: EDIT_POLICY_LABEL, exact: true }).click();
  return page.getByRole("dialog", { name: EDIT_POLICY });
}

test.describe("dialogs keep their footer reachable in a short window", () => {
  test.beforeEach(async ({ page }) => {
    await page.setViewportSize(SHORT_WINDOW);
    await signInAs(page, FIXTURE_KEY.operator);
  });

  test("the create-runner form scrolls to its Create button", async ({ page }) => {
    await signInAs(page, FIXTURE_KEY.policyEditor);
    await page.goto("/admin/runners");
    await page.getByRole("button", { name: CREATE_RUNNER, exact: true }).click();
    await expectFooterReachable(page.getByRole("dialog", { name: CREATE_RUNNER }), "Create");
  });

  test("the runner policy form scrolls to its Save button", async ({ page }) => {
    await expectFooterReachable(await openFirstRunnerPolicy(page), "Save");
  });

  test("the fleet-library edit form scrolls to its Save button", async ({ page }) => {
    await withPlatformEntry(async (name) => {
      await page.goto("/admin/fleet-libraries");
      await page.getByRole("row").filter({hasText: name}).getByRole("button", { name: EDIT, exact: true }).click();
      await expect(page.getByRole("dialog", { name: EDIT_TITLE })).toBeVisible();
      await page.setViewportSize(LIBRARY_SHORT_WINDOW);
      await expectFooterReachable(page.getByRole("dialog", { name: EDIT_TITLE }), "Save");
    });
  });
});

test.describe("measured placement", () => {
  test("the runner policy's isolation tiers sit side by side on one row", async ({ page }) => {
    await page.setViewportSize(WIDE_WINDOW);
    const dialog = await openFirstRunnerPolicy(page);
    const tiers = dialog.getByRole("radiogroup", { name: /isolation/i }).getByRole("radio");
    await expect(tiers.first()).toBeVisible({ timeout: RENDER_TIMEOUT_MS });
    const tops = await tiers.evaluateAll((radios) => radios.map((radio) => radio.getBoundingClientRect().top));
    expect(tops.length).toBeGreaterThan(1);
    expect(new Set(tops).size).toBe(1);
  });

  test("Library inspection stays inside a phone viewport", async ({ page }) => {
    await page.setViewportSize(PHONE_WINDOW);
    const workspaceId = await layoutWorkspaceId();
    await seedLibraryEntry(workspaceId, GALLERY_TEMPLATE);
    await signInAs(page, FIXTURE_KEY.regular);
    await page.goto(workspaceHref(workspaceId, WORKSPACE_LIBRARY));
    const row = page.getByRole("listitem").filter({hasText: GALLERY_TEMPLATE});
    await row.getByRole("button", {name: "Inspect", exact: true}).click();
    const start = page.getByRole("button", {name: "Start chat from this revision"});
    await expect(start).toBeVisible({ timeout: RENDER_TIMEOUT_MS });
    expect(await start.evaluate((element) => element.getBoundingClientRect().right)).toBeLessThanOrEqual(PHONE_WINDOW.width);
    const sideways = await page.evaluate(() => document.documentElement.scrollWidth - document.documentElement.clientWidth);
    expect(sideways).toBeLessThanOrEqual(LINE_TOLERANCE_PX);
  });
});

test.describe("a paginating table fills the page it sits on", () => {
  test.beforeEach(async ({ page }) => {
    await page.setViewportSize(TALL_WINDOW);
  });

  test("Secrets and Events fill their pages", async ({ page }) => {
    const workspaceId = await getDefaultWorkspaceId(FIXTURE_KEY.regular);
    const client = clientFor(FIXTURE_KEY.regular);
    // Secrets shows an empty state instead of a table until it holds a row.
    const secrets = `/v1/workspaces/${workspaceId}/secrets`;
    const secret = `e2e-table-fill-${crypto.randomUUID()}`;
    await client.post(secrets, { name: secret, data: { purpose: "table fill fixture" } });
    const fleet = await seedFleet(FIXTURE_KEY.regular, workspaceId, { name: `${TABLE_FILL_FLEET_PREFIX}${crypto.randomUUID()}` });
    try {
      await waitForFleetActive(FIXTURE_KEY.regular, workspaceId, fleet.id);
      await client.post(`/v1/workspaces/${workspaceId}/fleets/${fleet.id}/messages`, {
        message: TABLE_FILL_MESSAGE,
        operation_id: v7(),
      });
      await signInAs(page, FIXTURE_KEY.regular);
      for (const surface of ["secrets", "events"]) {
        await expectTableFillsPage(page, workspaceHref(workspaceId, surface));
      }
    } finally {
      await client.delete(`${secrets}/${encodeURIComponent(secret)}`);
      await cleanWorkspaceFleets(FIXTURE_KEY.regular, workspaceId, TABLE_FILL_FLEET_PREFIX);
    }
  });
});

test.describe("the platform Fleet library table ends at its rows", () => {
  test.beforeEach(async ({ page }) => {
    await page.setViewportSize(LIBRARY_WINDOW);
  });

  test("Platform > Fleet library", async ({ page }) => {
    await withPlatformEntry(async () => {
      await signInAs(page, FIXTURE_KEY.operator);
      await expectTableEndsAtRows(page, "/admin/fleet-libraries");
    });
  });
});
