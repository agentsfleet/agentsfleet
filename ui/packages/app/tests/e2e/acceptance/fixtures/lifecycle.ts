/** Shared lifecycle selectors retain workspace scope and verify read-only archived history. */
import { expect, type Page, type TestInfo } from "@playwright/test";
import {writeFile} from "node:fs/promises";
import {measureVisualState} from "../../fixtures/visual-measurement";
import {visualProvenance} from "../../fixtures/visual-provenance";
import {fleetNavigation} from "./nav";

const ROW_STATE_TIMEOUT_MS = 15_000;

type RowState = "active" | "installing" | "archived";

async function confirmAction(page: Page, label: "Archive"): Promise<void> {
  await page.getByRole("button", {name: "Fleet actions", exact: true}).click();
  await page.getByRole("menuitem", {name: label, exact: true}).click();
  const dialog = page.getByRole("alertdialog");
  await expect(dialog).toBeVisible();
  // Wait for the confirmed Server Action before leaving the current route.
  // Without this wait a spec that navigates right after aborts the in-flight
  // action POST and the fleet never actually changes state.
  const actionSettled = page.waitForResponse(
    (response) =>
      response.request().method() === "POST" &&
      response.request().headers()["next-action"] !== undefined,
    { timeout: ROW_STATE_TIMEOUT_MS },
  );
  await dialog.getByRole("button", { name: label }).click();
  await actionSettled;
  await expect(dialog).toBeHidden({ timeout: ROW_STATE_TIMEOUT_MS });
}

export async function archiveFleet(page: Page): Promise<void> {
  await confirmAction(page, "Archive");
  const notice = page.getByTestId("fleet-notice");
  await expect(notice).toContainText("Fleet archived");
  await page.getByRole("link", {name: "View history", exact: true}).click();
  // At 390 px the notice covers Fleet actions, and hovering it holds it open.
  await notice.getByRole("button", {name: "Dismiss notification", exact: true}).click();
  await expect(notice).toHaveAttribute("aria-hidden", "true");
}

export async function expectSidebarFleetState(
  page: Page,
  fleetId: string,
  state: RowState,
): Promise<void> {
  const navigation = await fleetNavigation(page);
  const row = navigation.locator(`a[href$="/fleets/${fleetId}"]`).filter({visible: true});
  await expect(row).toBeVisible();
  await expect(row).toHaveAttribute("title", new RegExp(` · ${state}$`), {
    timeout: ROW_STATE_TIMEOUT_MS,
  });
  if (state === "archived") await expect(row).toContainText("Archived");
  if (await page.getByRole("dialog", {name: "Navigation", exact: true}).isVisible()) await page.keyboard.press("Escape");
}

// Archived history permits permanent deletion without restoring execution.
const SUMMARY_BREAKPOINT_PX = 681;

export async function expectDetailArchived(page: Page): Promise<void> {
  // The Summary's Status says so at every width; the header label shows only
  // from the `summary` breakpoint, 42.5625rem (design-system theme.css).
  await expect(page.getByRole("region", {name: "Status", exact: true})).toContainText("Archived", {timeout: ROW_STATE_TIMEOUT_MS});
  if ((page.viewportSize()?.width ?? 0) >= SUMMARY_BREAKPOINT_PX) await expect(page.getByLabel("Fleet status: archived")).toBeVisible();
  await expect(page.getByRole("textbox")).toHaveCount(0);
  await expect(page.getByRole("button", {name: "Approve", exact: true})).toHaveCount(0);
  await expect(page.getByRole("button", {name: "Deny", exact: true})).toHaveCount(0);
  await page.getByRole("button", {name: "Fleet actions", exact: true}).click();
  await expect(page.getByRole("menuitem", {name: "Permanently delete", exact: true})).toBeVisible();
  for (const name of ["Stop", "Resume", "Restore", "Archive"]) await expect(page.getByRole("menuitem", {name, exact: true})).toHaveCount(0);
  await page.keyboard.press("Escape");
}

/** Called only after the journey confirms the backend transition. */
export async function captureLifecycleReview(page: Page, info: TestInfo, outcome: "archived" | "deleted") {
  const source = await visualProvenance();
  for (const width of [1480, 390]) for (const theme of ["dark", "light"]) {
    await page.setViewportSize({width, height: width === 390 ? 844 : 1000});
    await page.evaluate(value => {
      localStorage.setItem("theme", value);
      window.dispatchEvent(new StorageEvent("storage", {key: "theme", newValue: value}));
    }, theme);
    await expect(page.locator("html")).toHaveAttribute("data-theme", theme);
    await page.evaluate(() => document.fonts.ready);
    if (outcome === "archived") await expectDetailArchived(page);
    else {
      await expect(page).toHaveURL(/\/fleets\/new$/);
      await expect(page.getByTestId("fleet-thread-root")).toHaveCount(0);
    }
    const name = `authenticated-fleet-${outcome}-${width}-${theme}`;
    const geometry = await measureVisualState(page);
    await page.screenshot({path: info.outputPath(`${name}.png`), animations: "disabled"});
    await writeFile(info.outputPath(`${name}-evidence.json`), JSON.stringify({...source,
      captureTimeUtc: new Date().toISOString(), evidenceKind: "authenticated-lifecycle", outcome, width, theme, geometry, url: page.url()}, null, 2));
  }
  const after = await visualProvenance();
  expect(after.revision).toBe(source.revision);
  expect(after.sourceTreeSha256).toBe(source.sourceTreeSha256);
}
