/**
 * secrets-lifecycle.spec.ts — a secret, from the dialog that adds it to the
 * one that deletes it.
 *
 * Drives the Secrets page the way an operator does: add a secret through the
 * dialog, see that no fleet holds it, replace the value and delete the secret.
 * A fleet asking for a secret, and the page naming that fleet once a person
 * approves, is the careful-squirrel prime journey's. Cleanup runs against the
 * API whatever step fails.
 */
import * as crypto from "node:crypto";
import { expect, test, type Page } from "@playwright/test";
import { signInAs } from "./fixtures/auth";
import { clientFor } from "./fixtures/api-client";
import { getDefaultWorkspaceId } from "./fixtures/seed";
import { FIXTURE_KEY } from "./fixtures/constants";
import { gotoWorkspace } from "./fixtures/nav";

const ACTION_TIMEOUT_MS = 15_000;
// Three dialogs against a remote deployment, with the sign-in in front of them.
const JOURNEY_TIMEOUT_MS = 120_000;
const ACCESS_COLUMN = 3;
// The secret names a host the egress allowlist reads; no run sends it anywhere.
const SECRET_HOST = "example.com";

const suffix = () => crypto.randomBytes(3).toString("hex");

async function secretRow(page: Page, name: string) {
  await gotoWorkspace(page, FIXTURE_KEY.regular, "secrets");
  const row = page.getByRole("table").getByRole("row").filter({ hasText: name });
  await expect(row).toBeVisible({ timeout: ACTION_TIMEOUT_MS });
  return row;
}

async function manage(page: Page, name: string, action: "Replace value" | "Delete secret") {
  await page.getByRole("button", { name: `Manage ${name}`, exact: true }).click();
  const dialog = page.getByRole("dialog");
  await dialog.getByRole("button", { name: action, exact: true }).click();
}

test.describe("secrets lifecycle", () => {
  let workspaceId = "";
  let secretName: string | null = null;

  test.afterEach(async () => {
    if (secretName) {
      await clientFor(FIXTURE_KEY.regular)
        .delete(`/v1/workspaces/${workspaceId}/secrets/${encodeURIComponent(secretName)}`)
        .catch(() => undefined);
    }
    secretName = null;
  });

  test("an operator adds a secret no fleet holds, replaces its value and deletes it", async ({ page }) => {
    test.setTimeout(JOURNEY_TIMEOUT_MS);
    workspaceId = await getDefaultWorkspaceId(FIXTURE_KEY.regular);
    const name = `e2e_secret_${suffix()}`;
    secretName = name;
    await signInAs(page, FIXTURE_KEY.regular);
    await gotoWorkspace(page, FIXTURE_KEY.regular, "secrets");
    await expect(page.getByRole("heading", { name: /^secrets$/i })).toBeVisible();

    // Add, with named fields, through the dialog.
    await page.getByRole("button", { name: "Add secret", exact: true }).click();
    const add = page.getByRole("dialog");
    await add.getByLabel("Name", { exact: true }).fill(name);
    await add.getByRole("button", { name: "Use named fields" }).click();
    await add.getByLabel("Field 1 name").fill("api_key");
    await add.getByLabel("Field 1 value").fill("FIXTURE_TOKEN");
    await add.getByRole("button", { name: "Add field" }).click();
    await add.getByLabel("Field 2 name").fill("host");
    await add.getByLabel("Field 2 value").fill(SECRET_HOST);
    await add.getByRole("button", { name: "Add", exact: true }).click();
    await expect(add).toBeHidden({ timeout: ACTION_TIMEOUT_MS });

    // Nothing holds it: a recorded "none", not an unknown.
    const row = await secretRow(page, name);
    await expect(row.getByRole("cell").nth(ACCESS_COLUMN)).toHaveText("No active fleets");

    // Replace keeps the name and hides the value.
    await manage(page, name, "Replace value");
    const replace = page.getByRole("dialog", { name: "Replace secret" });
    await expect(replace.getByLabel("Name", { exact: true })).toHaveValue(name);
    await expect(replace.getByLabel("Name", { exact: true })).toHaveAttribute("readonly", "");
    await expect(replace.getByLabel("Field 1 value")).toHaveAttribute("type", "password");
    await replace.getByLabel("Field 1 name").fill("api_key");
    await replace.getByLabel("Field 1 value").fill("ROTATED_TOKEN");
    await replace.getByRole("button", { name: "Replace value", exact: true }).click();
    await expect(replace).toBeHidden({ timeout: ACTION_TIMEOUT_MS });
    await expect(page.getByRole("table").getByRole("row").filter({ hasText: name })).toBeVisible();

    // Delete warns that fleets referencing it will break, then removes it.
    await manage(page, name, "Delete secret");
    const confirm = page.getByRole("alertdialog");
    await expect(confirm).toContainText("Deleting breaks fleets that reference it.");
    await confirm.getByRole("button", { name: "Delete", exact: true }).click();
    await expect(page.getByRole("table").getByRole("row").filter({ hasText: name })).toHaveCount(0, { timeout: ACTION_TIMEOUT_MS });
    secretName = null;
  });
});
