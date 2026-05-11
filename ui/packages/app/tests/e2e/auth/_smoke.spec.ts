/**
 * WS-A.3 smoke — full auth wire: Clerk JWT + tenant bootstrap.
 *
 * Asserts:
 *   1. globalSetup ran (fail-fast didn't throw — implicit by reaching here).
 *   2. The fixture-JWT cache file exists with both fixture users' JWTs.
 *   3. signInAs(page, 'regular') mounts the __session cookie and Clerk
 *      middleware accepts the JWT (a navigation to '/' does NOT redirect to
 *      /sign-in).
 *   4. Post-bootstrap, the dashboard renders authenticated content for the
 *      signed-in fixture user (a marker like usezombie/Zombies/Dashboard is
 *      visible on body, not just the marketing/sign-in page).
 *
 * Per-spec teardown for fixture rows (zombies/credentials/events) lands with
 * the WS-C spec workstream — the bootstrap state itself is reused across
 * runs (idempotent on the user.created replay).
 */
import * as fs from "node:fs";
import * as path from "node:path";
import { expect, test } from "@playwright/test";
import { signInAs } from "./fixtures/auth";
import { getDefaultWorkspaceId, listZombies, seedZombie } from "./fixtures/seed";
import { cleanWorkspaceZombies } from "./fixtures/teardown";
import { FIXTURE_KEY } from "./fixtures/constants";

const JWT_CACHE_PATH = path.join(process.cwd(), ".fixture-jwts.json");

test.describe("auth e2e wire", () => {
  test("dashboard /sign-in renders", async ({ page }) => {
    await page.goto("/sign-in");
    const heading = page.getByRole("heading", { level: 1, name: /sign in/i });
    await expect(heading).toBeVisible();
  });

  test("required Clerk credentials present in env", () => {
    expect(process.env.NEXT_PUBLIC_API_URL?.length ?? 0).toBeGreaterThan(0);
    expect(process.env.CLERK_SECRET_KEY?.length ?? 0).toBeGreaterThan(20);
    expect(process.env.CLERK_WEBHOOK_SECRET?.length ?? 0).toBeGreaterThan(20);
  });

  test("globalSetup cached fixture JWTs for both fixture users", () => {
    expect(fs.existsSync(JWT_CACHE_PATH)).toBe(true);
    const cache = JSON.parse(fs.readFileSync(JWT_CACHE_PATH, "utf8")) as Record<
      string,
      { sessionJwt: string }
    >;
    expect(cache.regular?.sessionJwt?.length ?? 0).toBeGreaterThan(20);
    expect(cache.admin?.sessionJwt?.length ?? 0).toBeGreaterThan(20);
  });

  test("signInAs('regular') produces an accepted Clerk session", async ({ page }) => {
    await signInAs(page, FIXTURE_KEY.regular);
    await page.goto("/zombies");
    await expect(page).toHaveURL(/\/zombies(\?|$)/);
  });

  test(
    "post-bootstrap dashboard renders authenticated content for fixture user",
    async ({ page }) => {
      await signInAs(page, FIXTURE_KEY.regular);
      await page.goto("/zombies");
      await expect(page).toHaveURL(/\/zombies(\?|$)/);
      await expect(page.getByRole("heading", { name: /zombies/i }).first()).toBeVisible();
    },
  );

  test("seed + teardown roundtrip: create, list, delete the freshly-seeded zombie", async () => {
    const ws = await getDefaultWorkspaceId(FIXTURE_KEY.regular);

    // Seed a fresh zombie. Assertions only reference this row's id; pre-
    // existing rows from prior interrupted runs are noise we don't fail on.
    // Random suffix avoids (workspace_id, name) uniqueness collision when a
    // prior interrupted run left a killed zombie that couldn't be deleted
    // (zombied has a known ConnectionBusy bug on delete; orphans stick).
    const tag = Math.random().toString(36).slice(2, 8);
    const seeded = await seedZombie(FIXTURE_KEY.regular, ws, {
      name: `fixture-roundtrip-${tag}`,
    });
    expect(seeded.id).toBeTruthy();

    const after = await listZombies(FIXTURE_KEY.regular, ws);
    expect(after.some((z) => z.id === seeded.id)).toBe(true);

    // Teardown is tolerant of stale rows; we don't assert on the count
    // returned. Proof point: the freshly-seeded row is gone OR has been
    // marked killed. The "OR killed" branch accommodates an open zombied
    // bug where DELETE returns UZ-INTERNAL-002 (ConnectionBusy) — out of
    // scope here, tracked as a separate fix(zombie) PR. PATCH→killed is
    // what the harness can guarantee against that bug; once a future
    // fix(zombie) PR lands, this OR-clause can be removed.
    await cleanWorkspaceZombies(FIXTURE_KEY.regular, ws);

    const post = await listZombies(FIXTURE_KEY.regular, ws);
    const lingering = post.find((z) => z.id === seeded.id);
    expect(lingering === undefined || lingering.status === "killed").toBe(true);
  });
});
