/**
 * Live Clerk webhook signup smoke.
 *
 * This spec intentionally does NOT replay the identity event through the
 * harness. Clerk creates the browser signup session, Clerk delivers the real
 * `user.created` webhook to the configured backend, and the test waits for
 * the backend's Clerk metadata writeback before checking the dashboard.
 *
 * Runs against every deployed stack the acceptance lane targets. Skipped:
 * - locally, which cannot receive Clerk's hosted webhook without a tunnel.
 * - against production unless opted in, since signup there mutates real
 *   identity and tenant data.
 */
import * as crypto from "node:crypto";
import { expect, test } from "@playwright/test";
import { deleteUser, findUserIdByEmail } from "./fixtures/clerk-admin";
import { signUpAs } from "./fixtures/signup";
import { workspaceHref, workspaceUrlPattern } from "./fixtures/nav";

const PASSWORD = "SignupFixture!2026-live";
const PROD_WEBHOOK_FLAG = "E2E_ALLOW_PROD_SIGNUP_WEBHOOK";
const FLOW_TIMEOUT_MS = 90_000;

function uniqueEmail(): string {
  const tag = crypto.randomBytes(4).toString("hex");
  return `signup-webhook-${tag}+clerk_test@e2e.agentsfleet.net`;
}

const apiUrl = process.env.NEXT_PUBLIC_API_URL ?? "";
const isLocal = ["localhost", "127.0.0.1", "[::1]"].includes(new URL(process.env.BASE_URL ?? "http://localhost").hostname);
const isProdApi = apiUrl.includes("api.agentsfleet.net");
const prodWebhookEnabled = process.env[PROD_WEBHOOK_FLAG] === "1";

test.describe("signup webhook live", () => {
  test.skip(isLocal, "live webhook signup requires a public app URL");
  test.skip(isProdApi && !prodWebhookEnabled, `${PROD_WEBHOOK_FLAG}=1 is required for production`);
  test.setTimeout(FLOW_TIMEOUT_MS);

  let createdEmail: string | null = null;

  test.afterEach(async () => {
    if (!createdEmail) return;
    const userId = await findUserIdByEmail(createdEmail).catch(() => null);
    if (userId) await deleteUser(userId).catch((err: unknown) => {
        // Loud, not silent: a swallowed failure here is how users leak. The
        // global-teardown sweep is the backstop for anything that slips.
        console.error(`[e2e] fixture user cleanup failed for ${userId}:`, err);
      });
    createdEmail = null;
  });

  test("Clerk user.created webhook provisions workspace without harness bootstrap", async ({
    page,
  }) => {
    const email = uniqueEmail();
    createdEmail = email;

    const signup = await signUpAs(page, email, PASSWORD, {
      bootstrap: false,
      requireWorkspaceSession: true,
    });
    if (!signup.workspaceId) throw new Error("signup did not resolve a workspace id");

    await page.goto(workspaceHref(signup.workspaceId, "fleets"));
    await expect(page).toHaveURL(workspaceUrlPattern("fleets"));
    await expect(page.getByRole("heading", { name: "What should your fleet do?" })).toBeVisible();
    await expect(page.getByTestId("workspace-switcher")).toBeVisible();
    const createLink = page.getByRole("main").getByRole("link", { name: "New fleet", exact: true });
    await expect(createLink).toHaveCount(1);
    const createHref = workspaceHref(signup.workspaceId, "fleets/new");
    await expect(createLink).toHaveAttribute("href", createHref);
  });
});
