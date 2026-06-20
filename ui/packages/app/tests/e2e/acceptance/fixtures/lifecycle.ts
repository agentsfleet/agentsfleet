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
 * State assertions key on the dashboard listing's `data-state` attribute
 * (canonical mapping in app/(dashboard)/fleets/components/FleetsList.tsx:
 * active → live, paused/stopped → parked, killed/errored → failed).
 */
import { expect, type Page } from "@playwright/test";

const ROW_STATE_TIMEOUT_MS = 15_000;

type RowState = "live" | "parked" | "failed";

async function confirmAction(page: Page, label: "Stop" | "Resume" | "Kill"): Promise<void> {
  await page.getByRole("button", { name: label }).first().click();
  const dialog = page.getByRole("alertdialog");
  await expect(dialog).toBeVisible();
  await dialog.getByRole("button", { name: label }).click();
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
  const row = page.locator(`a[href="/fleets/${fleetId}"]`);
  await expect(row).toBeVisible();
  await expect(row).toHaveAttribute("data-state", state, {
    timeout: ROW_STATE_TIMEOUT_MS,
  });
}

// Terminal "Killed" indicator on the detail page once the fleet is killed.
// The action panel collapses to a disabled button labeled "Killed".
export async function expectDetailKilled(page: Page): Promise<void> {
  await expect(page.getByRole("button", { name: "Killed" })).toBeDisabled({
    timeout: ROW_STATE_TIMEOUT_MS,
  });
}
