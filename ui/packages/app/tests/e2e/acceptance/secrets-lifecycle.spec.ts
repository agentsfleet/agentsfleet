/**
 * secrets-lifecycle.spec.ts — custom-secret create (field builder, via dialog) →
 * list → rotate, on the standalone /secrets page.
 *
 * Secrets is a real standalone page again, not a
 * section on /settings/models. This drives the field/value AddSecretForm the
 * way a real operator would: open the "Create secret" dialog, fill a secret name
 * plus a field+value row, click Create secret, assert the row appears in the
 * DataTable-based SecretsList, then rotate it through the Edit dialog and
 * assert it is still listed under the same name. The UI has no standalone
 * delete assertion here (rotate only; rename is its own dialog now) — delete is covered elsewhere;
 * cleanup runs against the API in afterEach regardless of where the test
 * fails.
 */
import * as crypto from "node:crypto";
import { expect, test } from "@playwright/test";
import { signInAs } from "./fixtures/auth";
import { clientFor } from "./fixtures/api-client";
import { getDefaultWorkspaceId } from "./fixtures/seed";
import { FIXTURE_KEY } from "./fixtures/constants";
import { gotoWorkspace } from "./fixtures/nav";

const ACTION_TIMEOUT_MS = 15_000;

function uniqueName(): string {
  return `e2e-secret-${crypto.randomBytes(3).toString("hex")}`;
}

async function deleteSecretDirect(name: string): Promise<void> {
  const ws = await getDefaultWorkspaceId(FIXTURE_KEY.regular);
  const c = clientFor(FIXTURE_KEY.regular);
  await c
    .delete(`/v1/workspaces/${ws}/secrets/${encodeURIComponent(name)}`)
    .catch(() => undefined);
}

test.describe("secrets lifecycle", () => {
  let createdName: string | null = null;

  test.afterEach(async () => {
    if (createdName) await deleteSecretDirect(createdName);
    createdName = null;
  });

  test("operator adds a custom secret via the field builder, then rotates it", async ({ page }) => {
    const name = uniqueName();
    createdName = name;

    await signInAs(page, FIXTURE_KEY.regular);
    await gotoWorkspace(page, FIXTURE_KEY.regular, "secrets");
    await expect(page.getByRole("heading", { name: /^secrets$/i })).toBeVisible();

    // Add via the field/value builder, opened in a dialog.
    await page.getByRole("button", { name: "Create secret", exact: true }).click();
    const addDialog = page.getByRole("dialog");
    await expect(addDialog).toBeVisible();
    await addDialog.getByLabel("Secret name").fill(name);
    await addDialog.getByLabel("Field 1 name").fill("api_key");
    await addDialog.getByLabel("Field 1 value").fill("FLY_API_TOKEN");
    await addDialog.getByRole("button", { name: "Create secret", exact: true }).click();

    // Successful submit closes the dialog and refreshes the list.
    await expect(addDialog).toBeHidden({ timeout: ACTION_TIMEOUT_MS });
    const row = page.getByText(name, { exact: true }).first();
    await expect(row).toBeVisible({ timeout: ACTION_TIMEOUT_MS });

    // Rotate (overwrite in place) through the Edit dialog.
    await page.getByRole("button", { name: `Edit secret ${name}` }).click();
    const editDialog = page.getByRole("dialog");
    await expect(editDialog).toBeVisible();
    await editDialog.getByLabel("Data (JSON object)").fill('{"api_key":"ROTATED_TOKEN"}');
    await editDialog.getByRole("button", { name: "Rotate" }).click();
    await expect(editDialog).toBeHidden({ timeout: ACTION_TIMEOUT_MS });

    // Still listed under the same name after rotation.
    await expect(page.getByText(name, { exact: true }).first()).toBeVisible();
  });
});
