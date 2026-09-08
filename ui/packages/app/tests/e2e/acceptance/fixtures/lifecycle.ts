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
 * Actions live in the detail header ("Fleet lifecycle actions"), visible
 * from every rail view — the old Settings rail destination is gone.
 */
import { expect, type Page } from "@playwright/test";

const ROW_STATE_TIMEOUT_MS = 15_000;

type RowState = "live" | "parked" | "failed";

async function confirmAction(page: Page, label: "Stop" | "Resume" | "Kill"): Promise<void> {
  // KillSwitch renders its Stop/Resume/Kill buttons directly in the detail
  // header, so the action is reachable from whichever view the spec is on.
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
  //
  // `visible: true` is load-bearing, not defensive. During a router transition
  // React keeps the OUTGOING tree mounted and hidden while the incoming one
  // renders, so for a moment the document holds two walls and two anchors for
  // the same fleet. Playwright throws a strict-mode violation the instant a
  // locator resolves to more than one element — it does not retry past it — so
  // an assertion landing inside that window failed on a bare count mismatch
  // that named nothing, and passed on every run that landed outside it. That
  // is the whole of `login-install-lifecycle.spec.ts:45`'s intermittence: the
  // page was correct each time, and the locator was reading the tree React was
  // in the middle of retiring. Filtering to the visible one reads the wall the
  // person is actually looking at.
  const row = page.locator(`a[href$="/fleets/${fleetId}"]`).filter({ visible: true });
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
