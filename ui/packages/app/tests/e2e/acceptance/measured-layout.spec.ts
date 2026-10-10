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
 */
import { expect, type Locator, type Page, test } from "@playwright/test";
import { signInAs } from "./fixtures/auth";
import { FIXTURE_KEY } from "./fixtures/constants";
import { getDefaultWorkspaceId } from "./fixtures/seed";
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
    await page.goto("/admin/fleet-libraries");
    await page.getByRole("button", { name: EDIT, exact: true }).first().click();
    await expectFooterReachable(page.getByRole("dialog", { name: EDIT_TITLE }), "Save");
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
