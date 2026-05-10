/**
 * WS-A.2 smoke — proves Clerk admin-API JWT mint + signInAs cookie mount.
 *
 * Asserts:
 *   1. globalSetup ran (fail-fast didn't throw — implicit by reaching here).
 *   2. The fixture-JWT cache file exists with both fixture users' JWTs.
 *   3. signInAs(page, 'regular') mounts the __session cookie and Clerk
 *      middleware accepts the JWT (a navigation to '/' does NOT redirect to
 *      /sign-in).
 *
 * NOT yet asserted (lands with WS-A.3 once tenant bootstrap is wired):
 *   - dashboard renders the authenticated user's email
 *   - balance shows starter credit
 *   - workspace switcher lists the fixture user's workspace
 * Until bootstrap is in place, the dashboard may redirect post-Clerk to an
 * onboarding/empty state — Clerk auth is accepted but our DB has no tenant.
 */
import * as fs from "node:fs";
import * as path from "node:path";
import { expect, test } from "@playwright/test";
import { signInAs } from "./fixtures/auth";

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

  test("signInAs('regular') mounts a session cookie Clerk accepts", async ({ page }) => {
    await page.goto("/sign-in");
    await signInAs(page, "regular");
    await page.goto("/");
    expect(page.url()).not.toContain("/sign-in");
  });
});
