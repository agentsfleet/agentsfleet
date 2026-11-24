/**
 * Full-lifecycle Scenario 1 — ephemeral signup → workspace auto-provision →
 * a first message on New fleet → observe → bill → halt.
 *
 * End-to-end UI-driven: the operator never leaves the browser. signup goes
 * through Clerk's hosted form, install goes through /w/<id>/fleets/new, every
 * lifecycle transition is a real click in the dashboard. No API short-cuts.
 * A fleet is made by the message typed on New fleet; nothing is set up first.
 *
 * DEV-only — Clerk PROD almost certainly does not have test mode enabled, so
 * the `+clerk_test` alias would not short-circuit OTP. Mirrors
 * the same isProdApi guard signup.spec.ts uses.
 *
 * Why this spec exists: the persistent-fixture suite covers each lifecycle
 * slice individually but no spec walks a fresh operator from "I just signed
 * up" through to "I just archived a fleet I observed running." The dashboard's
 * route-guard chain (workspace auto-provision, starter credit, empty-state →
 * populated transition) only runs on a brand-new tenant — that surface is
 * uncovered without this spec.
 */
import * as crypto from "node:crypto";
import { expect, test } from "@playwright/test";
import { deleteUser, findUserIdByEmail } from "./fixtures/clerk-admin";
import { createFleetViaUI } from "./fixtures/new-fleet";
import { probeMessage } from "./fixtures/probe";
import {
  archiveFleet,
  expectDetailArchived,
  expectSidebarFleetState,
} from "./fixtures/lifecycle";
import { signUpAs } from "./fixtures/signup";
import { cleanWorkspaceFleets } from "./fixtures/teardown";
import { workspaceHref, workspaceUrlPattern } from "./fixtures/nav";

const PASSWORD = "SignupFixture!2026-stable";
const FLOW_TIMEOUT_MS = 120_000;

function uniqueEmail(): string {
  const tag = crypto.randomBytes(4).toString("hex");
  return `signup-lifecycle-${tag}+clerk_test@e2e.agentsfleet.net`;
}

const isProdApi = (process.env.NEXT_PUBLIC_API_URL ?? "").includes("api.agentsfleet.net");

test.describe("signup → first message → lifecycle", () => {
  test.skip(isProdApi, "Scenario 1 only runs against DEV/local — Clerk test mode is DEV-only");
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

  test("fresh signup walks a first message → observe → bill → halt entirely in the UI", {tag: "@post-deploy"}, async ({
    page,
  }) => {
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
    await expect(page.getByRole("heading", { name: "What should your fleet do?" })).toBeVisible();
    await expect(page.getByTestId("workspace-switcher")).toBeVisible();

    // The first message makes the fleet. The fresh tenant has exactly one
    // auto-provisioned workspace (signup.workspaceId) — the one active in the
    // browser — so the fleet is made there.
    const fleetId = await createFleetViaUI(page, workspaceId, probeMessage(crypto.randomBytes(4).toString("hex")));
    await expect(page).toHaveURL(workspaceUrlPattern(`fleets/${fleetId}`));
    // The detail page opens on Chat — the conversation card is the
    // post-install scaffolding assertion.
    await expect(page.getByLabel("Fleet chat")).toBeVisible({ timeout: 15_000 });

    // The sidebar lists the new fleet and its active status.
    await page.goto(workspaceHref(workspaceId, "fleets"));
    await expectSidebarFleetState(page, fleetId, "active");

    // Billing page loads the report's period totals.
    await page.goto("/settings/billing");
    await expect(page.getByLabel("Period totals")).toBeVisible();

    // Lifecycle: Archive via the AlertDialog confirm — terminal, history kept.
    await page.goto(workspaceHref(workspaceId, `fleets/${fleetId}`));
    await archiveFleet(page);
    await expectDetailArchived(page);
    await page.goto(workspaceHref(workspaceId, "fleets"));
    await expectSidebarFleetState(page, fleetId, "archived");
  });
});
