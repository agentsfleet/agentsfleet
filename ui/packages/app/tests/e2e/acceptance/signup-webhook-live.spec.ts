/**
 * Live Clerk webhook signup smoke.
 *
 * This spec intentionally does NOT replay the identity event through the
 * harness. Clerk creates the browser signup session, Clerk delivers the real
 * `user.created` webhook to the configured backend, and the test waits for
 * the backend's Clerk metadata writeback before checking the dashboard.
 *
 * Skipped by default:
 * - local cannot receive Clerk's hosted webhook without a tunnel.
 * - production signup mutates real identity + tenant data, so it requires a
 *   second explicit opt-in.
 */
import * as crypto from "node:crypto";
import { expect, test } from "@playwright/test";
import { deleteUser, findUserIdByEmail } from "./fixtures/clerk-admin";
import { signUpAs } from "./fixtures/signup";

const PASSWORD = "SignupFixture!2026-live";
const LIVE_WEBHOOK_FLAG = "E2E_LIVE_SIGNUP_WEBHOOK";
const PROD_WEBHOOK_FLAG = "E2E_ALLOW_PROD_SIGNUP_WEBHOOK";
const FLOW_TIMEOUT_MS = 90_000;

function uniqueEmail(): string {
  const tag = crypto.randomBytes(4).toString("hex");
  return `signup-webhook-${tag}+clerk_test@mailinator.com`;
}

const apiUrl = process.env.NEXT_PUBLIC_API_URL ?? "";
const isLocal = !process.env.BASE_URL;
const isProdApi = apiUrl.includes("api.usezombie.com");
const liveWebhookEnabled = process.env[LIVE_WEBHOOK_FLAG] === "1";
const prodWebhookEnabled = process.env[PROD_WEBHOOK_FLAG] === "1";

test.describe("signup webhook live", () => {
  test.skip(isLocal, "live webhook signup requires a public app URL");
  test.skip(!liveWebhookEnabled, `${LIVE_WEBHOOK_FLAG}=1 is required`);
  test.skip(isProdApi && !prodWebhookEnabled, `${PROD_WEBHOOK_FLAG}=1 is required for production`);
  test.setTimeout(FLOW_TIMEOUT_MS);

  let createdEmail: string | null = null;

  test.afterEach(async () => {
    if (!createdEmail) return;
    const userId = await findUserIdByEmail(createdEmail).catch(() => null);
    if (userId) await deleteUser(userId).catch(() => undefined);
    createdEmail = null;
  });

  test("Clerk user.created webhook provisions workspace without harness bootstrap", async ({
    page,
  }) => {
    const email = uniqueEmail();
    createdEmail = email;

    await signUpAs(page, email, PASSWORD, {
      bootstrap: false,
      requireWorkspaceSession: true,
    });

    await page.goto("/zombies");
    await expect(page).toHaveURL((url) => url.pathname === "/zombies");
    await expect(page.getByRole("heading", { name: /zombies/i }).first()).toBeVisible();
    await expect(page.getByTestId("workspace-switcher")).toBeVisible();
    await expect(page.getByText(/no zombies yet/i)).toBeVisible();
  });
});
