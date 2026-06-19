/**
 * credentials-lifecycle.spec.ts — /credentials create → list → delete.
 *
 * Drives the `AddCredentialForm` like a real operator: paste a name and a
 * JSON object, click Store, assert the new row appears in the list, then
 * click the per-row Delete button, confirm the dialog, assert the row is
 * gone. Covers a workspace-scoped surface (envelope-encrypted secrets)
 * that no other acceptance spec touches today.
 *
 * Cleanup is performed inside the test (the delete *is* the assertion),
 * but a defensive `afterEach` re-runs deletion against the API so a mid-
 * test failure cannot leak fixture credentials to the next run.
 */
import * as crypto from "node:crypto";
import { expect, test } from "@playwright/test";
import { signInAs } from "./fixtures/auth";
import { clientFor } from "./fixtures/api-client";
import { getDefaultWorkspaceId } from "./fixtures/seed";
import { FIXTURE_KEY } from "./fixtures/constants";

const ACTION_TIMEOUT_MS = 15_000;

function uniqueName(): string {
  return `e2e-cred-${crypto.randomBytes(3).toString("hex")}`;
}

async function deleteCredentialDirect(name: string): Promise<void> {
  const ws = await getDefaultWorkspaceId(FIXTURE_KEY.regular);
  const c = clientFor(FIXTURE_KEY.regular);
  await c
    .delete(`/v1/workspaces/${ws}/credentials/${encodeURIComponent(name)}`)
    .catch(() => undefined);
}

test.describe("credentials lifecycle", () => {
  let createdName: string | null = null;

  test.afterEach(async () => {
    if (createdName) await deleteCredentialDirect(createdName);
    createdName = null;
  });

  test("operator stores a credential then deletes it through the dialog", async ({ page }) => {
    const name = uniqueName();
    createdName = name;

    await signInAs(page, FIXTURE_KEY.regular);
    // The /credentials route redirects into the unified Models & Credentials
    // page; the add/list/delete surface lives in its Credentials section.
    await page.goto("/credentials");
    await expect(page).toHaveURL(/\/settings\/models/);
    await expect(page.getByRole("heading", { name: /credentials/i }).first()).toBeVisible();

    await page.getByText("Add a generic credential").click();
    await page.getByLabel("Name").fill(name);
    await page.getByLabel(/Data \(JSON object\)/).fill('{"host":"api.machines.dev","api_token":"FLY_API_TOKEN"}');
    await page.getByRole("button", { name: "Add", exact: true }).click();

    const row = page.getByText(name, { exact: true }).first();
    await expect(row).toBeVisible({ timeout: ACTION_TIMEOUT_MS });

    await page.getByRole("button", { name: `Delete credential ${name}` }).click();
    const dialog = page.getByRole("alertdialog");
    await expect(dialog).toBeVisible();
    await dialog.getByRole("button", { name: "Delete" }).click();
    await expect(dialog).toBeHidden({ timeout: ACTION_TIMEOUT_MS });

    await expect(page.getByText(name, { exact: true })).toHaveCount(0, {
      timeout: ACTION_TIMEOUT_MS,
    });
    createdName = null;
  });
});
