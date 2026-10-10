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
 * page gives it a definite height and the table claims it. Each half once
 * shipped broken alone (Events had a min-height shell, Platform > Fleet
 * library a table that never grew), so the table is measured against its page.
 */
import { expect, type Locator, type Page, test } from "@playwright/test";
import { signInAs } from "./fixtures/auth";
import { FIXTURE_KEY } from "./fixtures/constants";
import { clientFor } from "./fixtures/api-client";
import { getDefaultWorkspaceId, skillMd, triggerMd } from "./fixtures/seed";
import { SOURCE_KIND_UPLOAD } from "@/lib/types";
import { workspaceHref } from "./fixtures/nav";
import { EDIT, EDIT_TITLE } from "@/app/(dashboard)/admin/fleet-libraries/library-copy";
import { INSPECT_RUNNER_LABEL } from "@/app/(dashboard)/admin/runners/[runnerId]/components/runner-copy";

const SHORT_WINDOW = { width: 1280, height: 480 };
const WIDE_WINDOW = { width: 1280, height: 900 };
const PHONE_WINDOW = { width: 390, height: 844 };
const RENDER_TIMEOUT_MS = 15_000;
const CREATE_RUNNER = "Create runner";
const EDIT_POLICY = "Edit policy";
const LIBRARY_DESCRIPTION = '[data-testid^="library-card-description-"]';
// Sub-pixel rounding between a line box and its element's height.
const LINE_TOLERANCE_PX = 1;
const TALL_WINDOW = { width: 1280, height: 1400 };
const FULL_HEIGHT_PAGE = '[data-page-layout="full-height"]';
const DATA_TABLE = '[data-testid="data-table"]';
// Section padding between the table's frame and the page's bottom edge. A
// table that collapsed to its rows stops hundreds of pixels short in this window.
const FILL_GAP_PX = 48;
// A constant name: the global teardown sweep reaps fixture library rows.
const TABLE_FILL_TEMPLATE = "e2e-table-fill";
// Each test seeds the library rows it reads: on a fresh deployment no other
// project is guaranteed to have created one before this suite runs.
const PLATFORM_ENTRY_ID = "e2e-measured-layout";
const PLATFORM_LIBRARIES = "/v1/admin/fleet-libraries";
const GALLERY_TEMPLATE = "e2e-measured-gallery";

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

// Onboards one platform row for the operator's library page, and removes it after.
async function withPlatformEntry(body: () => Promise<void>): Promise<void> {
  const operator = clientFor(FIXTURE_KEY.operator);
  await operator.post(PLATFORM_LIBRARIES, {
    source_kind: SOURCE_KIND_UPLOAD,
    skill_markdown: skillMd(PLATFORM_ENTRY_ID),
    trigger_markdown: triggerMd(PLATFORM_ENTRY_ID),
  });
  try {
    await body();
  } finally {
    await operator.delete(`${PLATFORM_LIBRARIES}/${encodeURIComponent(PLATFORM_ENTRY_ID)}`);
  }
}

async function expectFooterReachable(dialog: Locator, footerButton: string): Promise<void> {
  await expect(dialog).toBeVisible({ timeout: RENDER_TIMEOUT_MS });
  // The form must overflow this window, or the reach below proves nothing.
  expect(await dialog.evaluate((element) => element.scrollHeight > element.clientHeight)).toBe(true);
  const footer = dialog.getByRole("button", { name: footerButton, exact: true });
  await footer.scrollIntoViewIfNeeded();
  await expect(footer).toBeInViewport();
}

async function openFirstRunnerPolicy(page: Page): Promise<Locator> {
  await page.goto("/admin/runners");
  const runner = page.getByRole("link", { name: new RegExp(`^${INSPECT_RUNNER_LABEL}`) }).first();
  await expect(runner).toBeVisible({ timeout: RENDER_TIMEOUT_MS });
  await runner.click();
  await page.getByRole("button", { name: EDIT_POLICY, exact: true }).click();
  return page.getByRole("dialog", { name: EDIT_POLICY });
}

test.describe("dialogs keep their footer reachable in a short window", () => {
  test.beforeEach(async ({ page }) => {
    await page.setViewportSize(SHORT_WINDOW);
    await signInAs(page, FIXTURE_KEY.operator);
  });

  test("the create-runner form scrolls to its Create button", async ({ page }) => {
    await page.goto("/admin/runners");
    await page.getByRole("button", { name: CREATE_RUNNER, exact: true }).click();
    await expectFooterReachable(page.getByRole("dialog", { name: CREATE_RUNNER }), "Create");
  });

  test("the runner policy form scrolls to its Save button", async ({ page }) => {
    await expectFooterReachable(await openFirstRunnerPolicy(page), "Save");
  });

  test("the fleet-library edit form scrolls to its Save button", async ({ page }) => {
    await withPlatformEntry(async () => {
      await page.goto("/admin/fleet-libraries");
      await page.getByRole("button", { name: EDIT, exact: true }).first().click();
      await expectFooterReachable(page.getByRole("dialog", { name: EDIT_TITLE }), "Save");
    });
  });
});

test.describe("measured placement", () => {
  test("the runner policy's isolation tiers sit side by side on one row", async ({ page }) => {
    await page.setViewportSize(WIDE_WINDOW);
    await signInAs(page, FIXTURE_KEY.operator);
    const dialog = await openFirstRunnerPolicy(page);
    const tiers = dialog.getByRole("radiogroup", { name: /isolation/i }).getByRole("radio");
    await expect(tiers.first()).toBeVisible({ timeout: RENDER_TIMEOUT_MS });
    const tops = await tiers.evaluateAll((radios) => radios.map((radio) => radio.getBoundingClientRect().top));
    expect(tops.length).toBeGreaterThan(1);
    expect(new Set(tops).size).toBe(1);
  });

  test("a gallery card's description stays one line however long it is", async ({ page }) => {
    await page.setViewportSize(PHONE_WINDOW);
    const workspaceId = await getDefaultWorkspaceId(FIXTURE_KEY.regular);
    await clientFor(FIXTURE_KEY.regular).post(`/v1/workspaces/${workspaceId}/fleet-libraries`, {
      source_kind: SOURCE_KIND_UPLOAD,
      skill_markdown: skillMd(GALLERY_TEMPLATE),
      trigger_markdown: triggerMd(GALLERY_TEMPLATE),
    });
    await signInAs(page, FIXTURE_KEY.regular);
    await page.goto(workspaceHref(workspaceId, "fleets/new"));
    const descriptions = page.locator(LIBRARY_DESCRIPTION);
    await expect(descriptions.first()).toBeVisible({ timeout: RENDER_TIMEOUT_MS });
    const overflow = await descriptions.evaluateAll((elements) =>
      elements.map((element) => {
        const style = getComputedStyle(element);
        const lineHeight = Number.parseFloat(style.lineHeight);
        return element.getBoundingClientRect().height - lineHeight;
      }),
    );
    expect(overflow.length).toBeGreaterThan(0);
    for (const extra of overflow) expect(extra).toBeLessThanOrEqual(LINE_TOLERANCE_PX);
  });
});

test.describe("a paginating table fills the page it sits on", () => {
  test.beforeEach(async ({ page }) => {
    await page.setViewportSize(TALL_WINDOW);
  });

  test("Configuration > Fleet library, Secrets and Events fill their pages", async ({ page }) => {
    const workspaceId = await getDefaultWorkspaceId(FIXTURE_KEY.regular);
    const client = clientFor(FIXTURE_KEY.regular);
    // Both lists show an empty state instead of a table until they hold a row.
    await client.post(`/v1/workspaces/${workspaceId}/fleet-libraries`, {
      source_kind: SOURCE_KIND_UPLOAD,
      skill_markdown: skillMd(TABLE_FILL_TEMPLATE),
      trigger_markdown: triggerMd(TABLE_FILL_TEMPLATE),
    });
    const secrets = `/v1/workspaces/${workspaceId}/secrets`;
    const secret = `e2e-table-fill-${crypto.randomUUID()}`;
    await client.post(secrets, { name: secret, data: { purpose: "table fill fixture" } });
    try {
      await signInAs(page, FIXTURE_KEY.regular);
      for (const surface of ["library", "secrets", "events"]) {
        await expectTableFillsPage(page, workspaceHref(workspaceId, surface));
      }
    } finally {
      await client.delete(`${secrets}/${encodeURIComponent(secret)}`);
    }
  });

  test("Platform > Fleet library fills its page", async ({ page }) => {
    await withPlatformEntry(async () => {
      await signInAs(page, FIXTURE_KEY.operator);
      await expectTableFillsPage(page, "/admin/fleet-libraries");
    });
  });
});
