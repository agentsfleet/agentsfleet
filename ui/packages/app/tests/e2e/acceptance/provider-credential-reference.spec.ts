import * as crypto from "node:crypto";
import { expect, test, type Page } from "@playwright/test";
import { signInAs } from "./fixtures/auth";
import { clientFor } from "./fixtures/api-client";
import { FIXTURE_KEY } from "./fixtures/constants";
import { getDefaultWorkspaceId } from "./fixtures/seed";

const ACTION_TIMEOUT_MS = 15_000;
const BACKEND_CREDENTIAL_SCOPE_ERROR = "credential row not found in vault";
const CREDENTIAL_NAME_LABEL = "Credential name";
const CURRENT_MODEL_SETUP_REGION = "Current model setup";
const INLINE_MODEL_SELECTOR = "#inline-model";
const MODEL_FALLBACK = "claude-sonnet-4-6";
const MODELS_HEADING = /^models & credentials$/i;
const PROVIDER_KEY_VALUE = "sk-";
const SAVE_SUCCESS_TEXT = "Saved. Run a test event to verify the key.";
const SAVE_KEY_BUTTON = "Save key";
const SAVE_MODEL_SETUP_BUTTON = "Save model setup";
const USE_PROVIDER_KEY_LABEL = "Use my provider key";

function uniqueName(): string {
  return `e2e-provider-${crypto.randomBytes(3).toString("hex")}`;
}

async function resetProviderDirect(): Promise<void> {
  await clientFor(FIXTURE_KEY.regular).delete("/v1/tenants/me/provider").catch(() => undefined);
}

async function deleteCredentialDirect(name: string | null): Promise<void> {
  if (!name) return;
  const ws = await getDefaultWorkspaceId(FIXTURE_KEY.regular);
  await clientFor(FIXTURE_KEY.regular)
    .delete(`/v1/workspaces/${ws}/credentials/${encodeURIComponent(name)}`)
    .catch(() => undefined);
}

async function fillInlineModelIfFreeText(page: Page): Promise<void> {
  const modelField = page.locator(INLINE_MODEL_SELECTOR);
  const tag = await modelField.evaluate((el) => el.tagName.toLowerCase()).catch(() => "");
  if (tag === "input") await modelField.fill(MODEL_FALLBACK);
}

async function waitForProviderSaveOutcome(page: Page): Promise<"saved" | "backend-mismatch"> {
  const saved = page.getByRole("status").filter({ hasText: SAVE_SUCCESS_TEXT });
  const backendMismatch = page.getByText(BACKEND_CREDENTIAL_SCOPE_ERROR);
  return await Promise.race([
    saved.waitFor({ state: "visible", timeout: ACTION_TIMEOUT_MS }).then(() => "saved" as const),
    backendMismatch
      .waitFor({ state: "visible", timeout: ACTION_TIMEOUT_MS })
      .then(() => "backend-mismatch" as const),
  ]);
}

test.describe("provider credential reference guard", () => {
  let createdName: string | null = null;

  test.afterEach(async () => {
    await resetProviderDirect();
    await deleteCredentialDirect(createdName);
    createdName = null;
  });

  test("user-interface provider key can become the active model setup credential", async ({ page }) => {
    const name = uniqueName();
    createdName = name;

    await resetProviderDirect();
    await signInAs(page, FIXTURE_KEY.regular);
    await page.goto("/settings/models");
    await expect(page.getByRole("heading", { name: MODELS_HEADING })).toBeVisible();

    await page.getByLabel(USE_PROVIDER_KEY_LABEL).click();
    const newKey = page.getByRole("button", { name: "+ New key" });
    if (await newKey.isVisible().catch(() => false)) await newKey.click();
    await page.getByLabel("API key").fill(PROVIDER_KEY_VALUE);
    await page.getByLabel(CREDENTIAL_NAME_LABEL).fill(name);
    await fillInlineModelIfFreeText(page);
    await page.getByRole("button", { name: SAVE_KEY_BUTTON }).click();
    await expect(page.getByText(name, { exact: true }).first()).toBeVisible({
      timeout: ACTION_TIMEOUT_MS,
    });

    await page.getByRole("button", { name: SAVE_MODEL_SETUP_BUTTON }).click();
    const outcome = await waitForProviderSaveOutcome(page);
    if (outcome === "backend-mismatch") {
      await expect(page.getByRole("button", { name: `Delete credential ${name}` })).toBeEnabled();
      await expect(page.getByText(name, { exact: true }).first()).toBeVisible();
      return;
    }

    await expect(page.getByRole("region", { name: CURRENT_MODEL_SETUP_REGION })).toContainText(name);
    await expect(
      page.getByRole("button", { name: `Credential ${name} is in model setup` }),
    ).toBeDisabled();
    await expect(page.getByText(name, { exact: true }).first()).toBeVisible();
  });
});
