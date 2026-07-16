/**
 * fleet-console.spec.ts — the operator lives on the three-column console
 * (M131 §3–§6). One page answers what the fleet IS (source), DOES (steer
 * thread), and KNOWS & COSTS (memory + runs ledger). This walks the whole
 * page the milestone exists to build: read the source, see a cost figure,
 * steer the fleet, then edit and save the source and read the next-wake
 * confirmation.
 *
 * Requires the full acceptance stack (seeded fleet + SSR auth + the live
 * PATCH …/fleets/{id} the source editor saves over). A regression lands as
 * a missing column region, a missing Source/Runs panel, or a save that
 * never leaves edit mode.
 */
import { expect, test } from "@playwright/test";
import { signInAs } from "./fixtures/auth";
import { FIXTURE_KEY } from "./fixtures/constants";
import { getDefaultWorkspaceId, seedFleet } from "./fixtures/seed";
import { cleanWorkspaceFleets } from "./fixtures/teardown";
import { workspaceHref, workspaceUrlPattern } from "./fixtures/nav";

const RENDER_TIMEOUT_MS = 15_000;

test.describe("fleet console", () => {
  test("test_e2e_operator_lives_on_the_console — read source, see cost, steer, edit + save", async ({ page }) => {
    const ws = await getDefaultWorkspaceId(FIXTURE_KEY.regular);
    const tag = Math.random().toString(36).slice(2, 8);
    const fleet = await seedFleet(FIXTURE_KEY.regular, ws, { name: `console-${tag}` });

    await signInAs(page, FIXTURE_KEY.regular);
    await page.goto(workspaceHref(ws, `fleets/${fleet.id}`));
    await expect(page).toHaveURL(workspaceUrlPattern(`fleets/${fleet.id}`));

    // The three labelled regions — what it IS / DOES / KNOWS & COSTS.
    await expect(page.getByRole("region", { name: "What it is" })).toBeVisible({ timeout: RENDER_TIMEOUT_MS });
    await expect(page.getByRole("region", { name: "What it does" })).toBeVisible();
    await expect(page.getByRole("region", { name: /What it knows/ })).toBeVisible();

    // Reads the source — the SKILL.md viewer in the left rail.
    const source = page.getByLabel("Source");
    await expect(source).toBeVisible();
    await expect(source.getByRole("tab", { name: "SKILL.md" })).toBeVisible();

    // Sees a cost — the runs ledger carries a lifetime spend figure (server
    // truth from budget_used_nanos), which renders even for a fresh fleet.
    const runs = page.getByLabel("Runs");
    await expect(runs).toBeVisible();
    await expect(runs.getByText(/\$\d/).first()).toBeVisible();

    // Steers the fleet — the reused thread + composer in the middle column.
    const thread = page.getByLabel("Live activity stream");
    await expect(thread).toBeVisible({ timeout: RENDER_TIMEOUT_MS });
    const composer = thread.getByLabel("Steer composer");
    const textarea = composer.getByPlaceholder(/steer this fleet/i);
    await expect(textarea).toBeVisible();
    await textarea.fill("console acceptance steer");
    await composer.getByRole("button", { name: /steer/i }).click();
    await expect(thread.getByText(/console acceptance steer/)).toBeVisible({ timeout: 5_000 });

    // Edits and saves the source — over the existing PATCH with next-wake
    // semantics.
    await source.getByRole("button", { name: /^Edit/ }).click();
    const editor = source.getByRole("textbox", { name: "Edit SKILL.md" });
    await expect(editor).toBeVisible();
    await editor.fill("# SKILL\n\nEdited by the console acceptance test.\n");
    await source.getByRole("button", { name: "Save changes" }).click();

    // The dialog states the exact next-wake contract before the operator commits.
    await expect(page.getByText(/Takes effect on the next wake/)).toBeVisible();
    await page.getByRole("button", { name: "Save" }).click();

    // The save lands: the editor leaves edit mode back to the viewer (no error),
    // proving the PATCH round-tripped.
    await expect(source.getByRole("button", { name: /^Edit/ })).toBeVisible({ timeout: RENDER_TIMEOUT_MS });
    await expect(source.getByRole("textbox", { name: "Edit SKILL.md" })).toHaveCount(0);
  });

  test.afterEach(async () => {
    const ws = await getDefaultWorkspaceId(FIXTURE_KEY.regular);
    await cleanWorkspaceFleets(FIXTURE_KEY.regular, ws);
  });
});
