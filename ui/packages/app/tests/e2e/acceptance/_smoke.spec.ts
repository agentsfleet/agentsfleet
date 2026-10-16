/**
 * WS-A.3 smoke — full auth wire: Clerk JWT + tenant bootstrap.
 *
 * Asserts:
 *   1. globalSetup ran (fail-fast didn't throw — implicit by reaching here).
 *   2. signInAs(page, 'regular') yields a session Clerk middleware accepts,
 *      and the dashboard renders authenticated content (the workspace
 *      switcher), not the sign-in page.
 *
 * Per-spec teardown for fixture rows (fleets/credentials/events) lands with
 * the WS-C spec workstream — the bootstrap state itself is reused across
 * runs (idempotent on the user.created replay).
 */
import { expect, test } from "@playwright/test";
import { setupClerkTestingToken } from "@clerk/testing/playwright";
import { signInAs } from "./fixtures/auth";
import { getDefaultWorkspaceId, listFleets, seedFleet } from "./fixtures/seed";
import { gotoWorkspace, workspaceUrlPattern } from "./fixtures/nav";
import { cleanWorkspaceFleets } from "./fixtures/teardown";
import { FIXTURE_KEY } from "./fixtures/constants";
import {
  assertCliArtifactPresent,
  diagnoseApiError,
  probeConnectorRegistry,
  probeDeployedService,
  probeRunnerHeartbeatAdvances,
  probeRunnerOnline,
  probeRuntimeModel,
} from "./fixtures/preflight";

// Clerk's SignIn card hydrates client-side from clerk.accounts.dev; a cold CI
// browser can take longer than expect()'s 5s default while the page itself is
// healthy. Web-first wait, not a sleep — passes as soon as the card mounts.
const CLERK_HYDRATION_TIMEOUT_MS = 15_000;

// Release preflight — every prerequisite a browser journey depends on, proven
// read-only before the expensive groups start. A failure here is an
// environment problem with a recovery step in the message, not product drift.
test.describe("release preflight", () => {
  test("deployed service is live and its dependencies are ready", async () => {
    await probeDeployedService();
  });

  test("a runtime model is available to tenants", async () => {
    await probeRuntimeModel();
  });

  test("the connector registry has a configured provider", async () => {
    await probeConnectorRegistry();
  });

  test("test_connector_proof_requires_online_runner", async () => {
    await probeRunnerOnline();
  });

  test("test_runner_last_seen_advances", async () => {
    await probeRunnerHeartbeatAdvances();
  });

  test("the workflow-built CLI artifact is present", () => {
    assertCliArtifactPresent();
  });
});

test.describe("auth e2e wire", () => {
  test("dashboard /sign-in renders", async ({ page }) => {
    // Same posture as every other browser path in this harness: the testing
    // token exempts clerk-js's Frontend API calls from bot protection, which
    // otherwise stalls the dev-instance card mount behind a challenge no
    // headless run can pass. The card itself still renders for real.
    await setupClerkTestingToken({ page });
    await page.goto("/sign-in");
    const heading = page.getByRole("heading", { level: 1, name: /sign in/i });
    await expect(heading).toBeVisible({ timeout: CLERK_HYDRATION_TIMEOUT_MS });
  });

  test(
    "post-bootstrap dashboard renders authenticated content for fixture user",
    async ({ page }) => {
      await signInAs(page, FIXTURE_KEY.regular);
      await gotoWorkspace(page, FIXTURE_KEY.regular, "fleets");
      await expect(page).toHaveURL(workspaceUrlPattern("fleets"));
      await expect(page.getByTestId("workspace-switcher")).toBeVisible();
    },
  );

  test("seed + teardown roundtrip: create, list, delete the freshly-seeded fleet", async () => {
    const ws = await getDefaultWorkspaceId(FIXTURE_KEY.regular);

    // Seed a fresh fleet. Assertions only reference this row's id; pre-
    // existing rows from prior interrupted runs are noise we don't fail on.
    // Random suffix avoids (workspace_id, name) uniqueness collision when a
    // prior interrupted run left a killed fleet that couldn't be deleted
    // (agentsfleetd has a known ConnectionBusy bug on delete; orphans stick).
    const tag = Math.random().toString(36).slice(2, 8);
    // The create path is also the QStash-visibility probe: a deployment whose
    // schedule credentials are unseeded 503s here (the cron trigger sync runs
    // inside fleet creation). Surface that as the typed recovery diagnosis
    // instead of a raw wire error the operator has to decode.
    const seeded = await seedFleet(FIXTURE_KEY.regular, ws, {
      name: `fixture-roundtrip-${tag}`,
    }).catch((error: unknown) => {
      throw diagnoseApiError(error, "fleet seed roundtrip");
    });
    expect(seeded.id).toBeTruthy();

    const after = await listFleets(FIXTURE_KEY.regular, ws);
    expect(after.some((z) => z.id === seeded.id)).toBe(true);

    const cleaned = await cleanWorkspaceFleets(FIXTURE_KEY.regular, ws, "fixture-roundtrip-");
    expect(cleaned.failed).toBe(0);
    const post = await listFleets(FIXTURE_KEY.regular, ws);
    expect(post.some((fleet) => fleet.id === seeded.id)).toBe(false);
  });
});
