/**
 * Full-lifecycle Scenario 1 — ephemeral signup → workspace auto-provision →
 * install via dashboard UI → observe → bill → halt.
 *
 * End-to-end UI-driven: the operator never leaves the browser. signup goes
 * through Clerk's hosted form, install goes through /w/<id>/fleets/new, every
 * lifecycle transition is a real click in the dashboard. No API short-cuts.
 *
 * DEV-only — Clerk PROD almost certainly does not have test mode enabled, so
 * the `+clerk_test@mailinator.com` alias would not short-circuit OTP. Mirrors
 * the same isProdApi guard signup.spec.ts uses.
 *
 * Why this spec exists: the persistent-fixture suite covers each lifecycle
 * slice individually but no spec walks a fresh operator from "I just signed
 * up" through to "I just killed a fleet I observed running." The dashboard's
 * route-guard chain (workspace auto-provision, starter credit, empty-state →
 * populated transition) only runs on a brand-new tenant — that surface is
 * uncovered without this spec.
 */
import * as crypto from "node:crypto";
import { expect, test } from "@playwright/test";
import { deleteUser, findUserIdByEmail } from "./fixtures/clerk-admin";
import { installViaUI } from "./fixtures/install-ui";
import {
  expectDetailKilled,
  expectRowState,
  killFleet,
  resumeFleet,
  stopFleet,
} from "./fixtures/lifecycle";
import { signUpAs } from "./fixtures/signup";
import { cleanWorkspaceFleets } from "./fixtures/teardown";
import { workspaceHref, workspaceUrlPattern } from "./fixtures/nav";

const PASSWORD = "SignupFixture!2026-stable";
const FLOW_TIMEOUT_MS = 120_000;

function uniqueEmail(): string {
  const tag = crypto.randomBytes(4).toString("hex");
  return `signup-lifecycle-${tag}+clerk_test@mailinator.com`;
}

function uniqueName(): string {
  return `lifecycle-${crypto.randomBytes(4).toString("hex")}`;
}

const isProdApi = (process.env.NEXT_PUBLIC_API_URL ?? "").includes("api.agentsfleet.net");

test.describe("signup → install → lifecycle", () => {
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
    if (userId) await deleteUser(userId).catch(() => undefined);
    createdEmail = null;
  });

  test("fresh signup walks install → observe → bill → halt entirely in the UI", async ({
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

    // Dashboard /w/<id>/fleets renders auto-provisioned workspace + empty state.
    // First-deploy regression surface lives here (route-guard chain on a
    // brand-new tenant, WorkspaceSwitcher with the auto-provisioned default).
    await page.goto(workspaceHref(workspaceId, "fleets"));
    await expect(page.getByRole("heading", { name: /fleets/i }).first()).toBeVisible();
    await expect(page.getByTestId("workspace-switcher")).toBeVisible();
    await expect(page.getByText(/no fleets yet/i)).toBeVisible();

    // Install via the dashboard template gallery. The fresh tenant has exactly
    // one auto-provisioned workspace (signup.workspaceId) — the one active in
    // the browser — so the onboard targets it and its card renders here. The
    // onboard reuses the signup session's own JWT (no persistent fixture).
    const name = uniqueName();
    const fleetId = await installViaUI(page, name, {
      handle: { sessionJwt: signup.sessionJwt },
      workspaceId,
    });

    // Post-install: the form redirects to /w/<id>/fleets/${id}. Recent Activity
    // section is the section-scaffolding assertion (matches logs-detail's
    // downgrade — section presence, not payload contents).
    await expect(page).toHaveURL(workspaceUrlPattern(`fleets/${fleetId}`));
    await expect(page.getByRole("region", { name: "Recent Activity" })).toBeVisible();

    // Listing shows the new row live.
    await page.goto(workspaceHref(workspaceId, "fleets"));
    await expectRowState(page, fleetId, "live");

    // Billing page renders the balance card (starter credit).
    await page.goto("/settings/billing");
    await expect(page.getByTestId("balance-headline")).toBeVisible();

    // Lifecycle: Stop → Resume → Kill, each via the AlertDialog confirm.
    await page.goto(workspaceHref(workspaceId, `fleets/${fleetId}`));
    await stopFleet(page);
    await page.goto(workspaceHref(workspaceId, "fleets"));
    await expectRowState(page, fleetId, "parked");

    await page.goto(workspaceHref(workspaceId, `fleets/${fleetId}`));
    await resumeFleet(page);
    await page.goto(workspaceHref(workspaceId, "fleets"));
    await expectRowState(page, fleetId, "live");

    await page.goto(workspaceHref(workspaceId, `fleets/${fleetId}`));
    await killFleet(page);
    await expectDetailKilled(page);
    await page.goto(workspaceHref(workspaceId, "fleets"));
    await expectRowState(page, fleetId, "failed");
  });
});
