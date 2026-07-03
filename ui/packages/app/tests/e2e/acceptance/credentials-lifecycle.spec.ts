/**
 * credentials-lifecycle.spec.ts — custom-secret add (field builder) → list →
 * rotate, on the consolidated Models page.
 *
 * After the M102 consolidation the standalone /credentials vault is gone; the
 * custom-secrets section now lives in the `custom-secrets-group` region on
 * /settings/models. This drives the field/value AddCredentialForm like a real
 * operator: a secret name plus a field+value row, click Add secret, assert the
 * row appears, then Replace (rotate) it through the Edit dialog and assert it
 * is still listed under the same name. The UI has no standalone delete
 * (rotate/rename only), so cleanup runs against the API in afterEach. All
 * form interactions are scoped to the custom-secrets group so they never
 * collide with the provider-key forms on the same page.
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

  test("operator adds a custom secret via the field builder, then rotates it", async ({ page }) => {
    const name = uniqueName();
    createdName = name;

    await signInAs(page, FIXTURE_KEY.regular);
    await page.goto("/settings/models");
    await expect(page.getByRole("heading", { name: /^models & keys$/i })).toBeVisible();

    // Scope every action to the custom-secrets group so the provider-key forms
    // elsewhere on the page can't shadow these labels.
    const secrets = page.getByTestId("custom-secrets-group");
    await expect(secrets).toBeVisible();

    // Add via the field/value builder: secret name + one field row.
    await secrets.getByLabel("Secret name").fill(name);
    await secrets.getByLabel("Field 1 name").fill("api_key");
    await secrets.getByLabel("Field 1 value").fill("FLY_API_TOKEN");
    await secrets.getByRole("button", { name: "Add secret", exact: true }).click();

    const row = secrets.getByText(name, { exact: true }).first();
    await expect(row).toBeVisible({ timeout: ACTION_TIMEOUT_MS });

    // Rotate (overwrite in place) through the Replace → Edit dialog.
    await secrets.getByRole("button", { name: `Replace secret ${name}` }).click();
    const dialog = page.getByRole("dialog");
    await expect(dialog).toBeVisible();
    await dialog.getByLabel("Data (JSON object)").fill('{"api_key":"ROTATED_TOKEN"}');
    await dialog.getByRole("button", { name: "Rotate" }).click();
    await expect(dialog).toBeHidden({ timeout: ACTION_TIMEOUT_MS });

    // Still listed under the same name after rotation.
    await expect(secrets.getByText(name, { exact: true }).first()).toBeVisible();
  });
});
