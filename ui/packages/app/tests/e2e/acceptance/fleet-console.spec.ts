/**
 * fleet-console.spec.ts — the operator lives on the fleet console: a local
 * navigation rail (Chat, Events, Memory, Skill, Trigger, Settings) beside one
 * working surface at a time, with Chat as the action surface.
 *
 * The walk proves the two things a console must get right before anything
 * else: the composer is reachable without scrolling the page even when the
 * fleet has a history, and a message actually leaves the composer. It then
 * checks that a non-chat view still behaves as ordinary page content, and
 * that the way back to the wall works.
 *
 * Requires the full acceptance stack (seeded fleet + SSR auth). A regression
 * lands as a composer below the fold, a message that never leaves the field,
 * or a clipped Skill view.
 */
import { expect, test } from "@playwright/test";
import { signInAs } from "./fixtures/auth";
import { FIXTURE_KEY } from "./fixtures/constants";
import { getDefaultWorkspaceId, seedFleet, waitForFleetActive } from "./fixtures/seed";
import { cleanWorkspaceFleets } from "./fixtures/teardown";
import { workspaceHref, workspaceUrlPattern } from "./fixtures/nav";

const RENDER_TIMEOUT_MS = 15_000;
const SEND_TIMEOUT_MS = 10_000;

test.describe("fleet console", () => {
  test("test_e2e_operator_lives_on_the_console — reach the composer, send, navigate", async ({
    page,
  }) => {
    const ws = await getDefaultWorkspaceId(FIXTURE_KEY.regular);
    const tag = Math.random().toString(36).slice(2, 8);
    const fleet = await seedFleet(FIXTURE_KEY.regular, ws, { name: `console-${tag}` });
    await waitForFleetActive(FIXTURE_KEY.regular, ws, fleet.id);

    await signInAs(page, FIXTURE_KEY.regular);
    await page.goto(workspaceHref(ws, `fleets/${fleet.id}`));
    await expect(page).toHaveURL(workspaceUrlPattern(`fleets/${fleet.id}`));

    // The console's local rail — one working surface at a time.
    const rail = page.getByRole("navigation", { name: "Fleet sections" });
    await expect(rail).toBeVisible({ timeout: RENDER_TIMEOUT_MS });
    // Five destinations — lifecycle actions moved to the detail header, so
    // Settings is no longer a rail view.
    for (const section of ["Chat", "Events", "Memory", "Skill", "Trigger"]) {
      await expect(rail.getByRole("link", { name: section })).toBeVisible();
    }

    const thread = page.getByLabel("Fleet chat");
    await expect(thread).toBeVisible({ timeout: RENDER_TIMEOUT_MS });

    // test_console_composer_is_reachable_without_page_scroll — the defect this
    // milestone exists for: the card used to grow to the height of its whole
    // history, so reaching the composer meant scrolling past every event.
    // The composer is pinned OUTSIDE the chat card — a sibling below it —
    // so it stays reachable while the transcript scrolls internally.
    const composer = page.getByLabel("Chat composer");
    await expect(composer).toBeInViewport();
    const pageOverflow = await page.evaluate(
      () => document.documentElement.scrollHeight - document.documentElement.clientHeight,
    );
    expect(pageOverflow).toBeLessThanOrEqual(1);

    // Nor does anything paint outside its track horizontally.
    const sideways = await page.evaluate(
      () => document.documentElement.scrollWidth - document.documentElement.clientWidth,
    );
    expect(sideways).toBeLessThanOrEqual(1);

    // The message list is the only thing that scrolls.
    const conversation = thread.getByRole("log");
    await expect(conversation).toBeVisible();

    // A message leaves the composer and lands in the conversation.
    const textarea = composer.getByPlaceholder(/message this fleet/i);
    await expect(textarea).toBeEnabled();
    await textarea.fill("console acceptance steer");
    await composer.getByRole("button", { name: /^Send/ }).click();
    await expect(thread.getByText("console acceptance steer")).toBeVisible({
      timeout: SEND_TIMEOUT_MS,
    });
    await expect(textarea).toHaveValue("");

    // test_non_chat_console_views_scroll_normally — the Skill view is ordinary
    // page content: it is reachable and nothing is clipped away.
    await rail.getByRole("link", { name: "Skill" }).click();
    const source = page.getByLabel("Source");
    await expect(source).toBeVisible({ timeout: RENDER_TIMEOUT_MS });
    await source.getByRole("button", { name: "View source" }).click();
    // The Skill view shows one document directly — no tab bar. Expanding
    // reveals the seeded skill body and flips the disclosure to Hide.
    await expect(source.getByRole("button", { name: "Hide source" })).toBeVisible();
    await expect(source.getByText("acceptance-seed").first()).toBeVisible();

    // The way back to the wall — the breadcrumb's first crumb, scoped to its
    // own landmark so it is not confused with the sidebar destination.
    await page.getByRole("navigation", { name: "Breadcrumb" })
      .getByRole("link", { name: "Fleets" })
      .click();
    await expect(page).toHaveURL(workspaceUrlPattern("fleets"));
  });

  test.afterEach(async () => {
    const ws = await getDefaultWorkspaceId(FIXTURE_KEY.regular);
    await cleanWorkspaceFleets(FIXTURE_KEY.regular, ws, "console-");
  });
});
