/**
 * signup-first-fleet.spec.ts — a person who just signed up has a workspace
 * that can hold a fleet.
 *
 * Sign-up goes through Clerk's browser flow; the dashboard's route-guard chain
 * (workspace auto-provision, starter credit, the empty state turning populated)
 * only runs on a brand-new tenant, and this is the spec that walks it. The
 * fleet is the prime set's first-message fleet, made through the shared helper
 * with the new person's own session: that a first message makes a fleet is the
 * prime journey's to prove, and archiving one is the archive spec's.
 *
 * DEV-only — Clerk PROD almost certainly does not have test mode enabled, so
 * the `+clerk_test` alias would not short-circuit OTP. Mirrors
 * the same isProdApi guard signup.spec.ts uses.
 */
import * as crypto from "node:crypto";
import { expect, test } from "@playwright/test";
import { deleteUser, findUserIdByEmail } from "./fixtures/clerk-admin";
import { expectSidebarFleetState } from "./fixtures/lifecycle";
import { createPrimeFleet, PRIME_FLEET, primeFleet } from "./fixtures/prime-fleets";
import { signUpAs } from "./fixtures/signup";
import { cleanWorkspaceFleets } from "./fixtures/teardown";
import { workspaceHref } from "./fixtures/nav";

const PASSWORD = "SignupFixture!2026-stable";
// Sign-up, then the prime fleet's first turn settling on a runner.
const FLOW_TIMEOUT_MS = 300_000;
const fixture = primeFleet(PRIME_FLEET.morningRooster);

function uniqueEmail(): string {
  const tag = crypto.randomBytes(4).toString("hex");
  return `signup-lifecycle-${tag}+clerk_test@e2e.agentsfleet.net`;
}

const isProdApi = (process.env.NEXT_PUBLIC_API_URL ?? "").includes("api.agentsfleet.net");

test.describe("a fresh signup's workspace", () => {
  test.skip(isProdApi, "runs against DEV and local only: Clerk test mode is DEV-only");
  test.setTimeout(FLOW_TIMEOUT_MS);

  let createdEmail: string | null = null;
  let cleanupSession: { sessionJwt: string; workspaceId: string } | null = null;

  test.afterEach(async () => {
    if (cleanupSession) {
      await cleanWorkspaceFleets(
        { sessionJwt: cleanupSession.sessionJwt },
        cleanupSession.workspaceId,
      ).catch(() => undefined);
      cleanupSession = null;
    }
    if (!createdEmail) return;
    const userId = await findUserIdByEmail(createdEmail).catch(() => null);
    if (userId) await deleteUser(userId).catch((err: unknown) => {
        // Loud, not silent: a swallowed failure here is how users leak. The
        // global-teardown sweep is the backstop for anything that slips.
        console.error(`[e2e] fixture user cleanup failed for ${userId}:`, err);
      });
    createdEmail = null;
  });

  test("a fresh signup lands in a workspace that opens New fleet, lists its first fleet and reads its bill", async ({page}) => {
    const email = uniqueEmail();
    createdEmail = email;

    // Clerk DEV's hosted SignUp form renders a Cloudflare Turnstile widget
    // on the email/password step that gates navigation to the OTP screen
    // even with the testing-token captcha-bypass in place. `signUpAs`
    // drives Clerk's browser SDK directly to skip the form — see
    // fixtures/signup.ts for why this is equivalent to a real signup.
    const signup = await signUpAs(page, email, PASSWORD, { requireWorkspaceSession: true });
    if (!signup.workspaceId) throw new Error("signup did not return a workspace id");
    const workspaceId = signup.workspaceId;
    cleanupSession = { sessionJwt: signup.sessionJwt, workspaceId };

    // A fresh workspace opens New fleet through its fleet entry route.
    // First-deploy regression surface lives here (route-guard chain on a
    // brand-new tenant, WorkspaceSwitcher with the auto-provisioned default).
    await page.goto(workspaceHref(workspaceId, "fleets"));
    await expect(page.getByRole("heading", { name: "New fleet" })).toBeVisible();
    await expect(page.getByTestId("workspace-switcher")).toBeVisible();

    // The fresh tenant has exactly one auto-provisioned workspace, the one
    // active in the browser, so the fleet is made there with its own session.
    const fleetId = (await createPrimeFleet({sessionJwt: signup.sessionJwt}, workspaceId, fixture)).id;

    // The sidebar lists the new fleet and its active status.
    await page.goto(workspaceHref(workspaceId, "fleets"));
    await expectSidebarFleetState(page, fleetId, "active");

    // Billing page loads the report's period totals.
    await page.goto("/settings/billing");
    await expect(page.getByLabel("Period totals")).toBeVisible();
  });
});
