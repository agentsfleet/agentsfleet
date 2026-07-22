/**
 * Shared selectors + action helpers for KillSwitch lifecycle transitions.
 *
 * The Stop / Resume / Kill flow is the same Radix AlertDialog wiring across
 * every spec that drives it: lifecycle, kill, and the two full-lifecycle
 * scenarios. Each action is a primary button on the detail page + a confirm
 * button inside an alertdialog role. Without a shared helper the four specs
 * duplicate the same getByRole pattern, and a future ConfirmDialog refactor
 * (button label, copy, dialog role) has to be tracked across four files.
 *
 * State assertions key on the wall tile anchor (FleetTile) `data-state`
 * attribute (canonical mapping in
 * app/(dashboard)/w/[workspaceId]/fleets/components/FleetTile.tsx:
 * active → live, paused/stopped → parked, killed/errored → failed).
 * Actions live in the fleet's Settings view behind the local rail.
 */
import { expect, type Page } from "@playwright/test";

const ROW_STATE_TIMEOUT_MS = 15_000;

type RowState = "live" | "parked" | "failed";

async function confirmAction(page: Page, label: "Stop" | "Resume" | "Kill"): Promise<void> {
  // The lifecycle controls live in the fleet's Settings view. The rail link
  // works from whichever view the spec landed on.
  await page
    .getByRole("navigation", { name: "Fleet sections" })
    .getByRole("link", { name: "Settings" })
    .click();
  await page.getByRole("button", { name: label }).first().click();
  const dialog = page.getByRole("alertdialog");
  await expect(dialog).toBeVisible();
  // The dialog and status flip optimistically the moment confirm is clicked;
  // only the Server Action's response proves the transition reached the API.
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

export async function stopFleet(page: Page): Promise<void> {
  await confirmAction(page, "Stop");
}

export async function resumeFleet(page: Page): Promise<void> {
  await confirmAction(page, "Resume");
}

export async function killFleet(page: Page): Promise<void> {
  await confirmAction(page, "Kill");
}

export async function expectRowState(
  page: Page,
  fleetId: string,
  state: RowState,
): Promise<void> {
  // The wall tile anchor (FleetTile) is workspace-scoped
  // (`/w/<workspaceId>/fleets/<id>`); match on the stable suffix so this shared
  // helper needn't thread the workspace id through every caller.
  const row = page.locator(`a[href$="/fleets/${fleetId}"]`);
  await expect(row).toBeVisible();
  await expect(row).toHaveAttribute("data-state", state, {
    timeout: ROW_STATE_TIMEOUT_MS,
  });
}

// A killed fleet exposes only its terminal cleanup action in the detail header.
export async function expectDetailKilled(page: Page): Promise<void> {
  await expect(page.getByRole("button", { name: "Delete fleet" })).toBeVisible({
    timeout: ROW_STATE_TIMEOUT_MS,
  });
  await expect(page.getByRole("button", { name: "Stop" })).toHaveCount(0);
  await expect(page.getByRole("button", { name: "Kill" })).toHaveCount(0);
}
