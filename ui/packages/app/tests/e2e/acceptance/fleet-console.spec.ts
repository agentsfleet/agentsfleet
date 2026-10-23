/**
 * fleet-console.spec.ts — the operator lives on the fleet console: a local
 * navigation rail (Chat, Activity, Memory) beside one
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
 * or a clipped Memory view.
 */
import { expect, test, type Page } from "@playwright/test";
import { signInAs } from "./fixtures/auth";
import { FIXTURE_KEY } from "./fixtures/constants";
import { getDefaultWorkspaceId, seedFleet, waitForFleetActive } from "./fixtures/seed";
import { cleanWorkspaceFleets } from "./fixtures/teardown";
import { workspaceHref, workspaceUrlPattern } from "./fixtures/nav";

const RENDER_TIMEOUT_MS = 15_000;
const SEND_TIMEOUT_MS = 10_000;
// An idle fleet first flushes through the proxy on the daemon's 15s heartbeat.
const STREAM_OPEN_TIMEOUT_MS = 30_000;
const FOLLOW_UP = "And report build errors";

test.describe("fleet console", () => {
  test("test_e2e_operator_lives_on_the_console — reach the composer, send, navigate", async ({
    page,
  }) => {
    const ws = await getDefaultWorkspaceId(FIXTURE_KEY.regular);
    const tag = Math.random().toString(36).slice(2, 8);
    const fleet = await seedFleet(FIXTURE_KEY.regular, ws, { name: `console-${tag}` });
    await waitForFleetActive(FIXTURE_KEY.regular, ws, fleet.id);

    await signInAs(page, FIXTURE_KEY.regular);
    await visitFleetAndCheckStream(page, ws, fleet.id);
    await expect(page).toHaveURL(workspaceUrlPattern(`fleets/${fleet.id}`));

    // The console's local rail — one working surface at a time.
    const rail = page.getByRole("navigation", { name: "Fleet sections" });
    await expect(rail).toBeVisible({ timeout: RENDER_TIMEOUT_MS });
    for (const section of ["Chat", "Activity", "Memory"]) {
      await expect(rail.getByRole("link", { name: section })).toBeVisible();
    }
    await expect(rail.getByRole("link", {name: "Setup", exact: true})).toHaveCount(0);
    await expect(page.getByRole("button", {name: "Edit setup", exact: true})).toBeVisible();

    const thread = page.getByTestId("fleet-thread-root");
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
    const textarea = composer.getByPlaceholder(/message your fleet/i);
    await expect(textarea).toBeEnabled();
    await expect(textarea).toBeFocused();
    await textarea.fill("console acceptance steer");
    await composer.getByRole("button", { name: /^Send/ }).click();
    await expect(thread.getByText("console acceptance steer")).toBeVisible({
      timeout: SEND_TIMEOUT_MS,
    });
    await expect(textarea).toHaveValue("");
    await expect(textarea).toBeFocused();
    await page.keyboard.type(FOLLOW_UP);
    await expect(textarea).toHaveValue(FOLLOW_UP);

    await rail.getByRole("link", { name: "Memory", exact: true }).click();
    await expect(page).toHaveURL(new RegExp(`fleets/${fleet.id}\\?view=memory$`));
    await expect(page.getByRole("button", {name: "Edit setup", exact: true})).toBeVisible();

    // The shared sidebar returns to all saved fleets from every fleet section.
    await page.getByRole("navigation", { name: "Primary" })
      .getByRole("link", { name: "Your fleets", exact: true })
      .click();
    await expect(page).toHaveURL(workspaceUrlPattern("fleets"));
  });

  test.afterEach(async () => {
    const ws = await getDefaultWorkspaceId(FIXTURE_KEY.regular);
    await cleanWorkspaceFleets(FIXTURE_KEY.regular, ws, "console-");
  });
});

/** Observe the page's own stream; this proves only the browser-to-app hop. */
async function visitFleetAndCheckStream(page: Page, workspaceId: string, fleetId: string) {
  const target = new URL(workspaceHref(workspaceId, `fleets/${fleetId}`), test.info().project.use.baseURL);
  const streamPath = `/live/v1/workspaces/${workspaceId}/fleets/${fleetId}/events/stream`;
  const session = await page.context().newCDPSession(page);
  let observed: { status: number; protocol: string | null; mimeType: string } | undefined;
  try {
    await session.send("Network.enable");
    session.on("Network.responseReceived", ({ response, type }) => {
      if (type !== "EventSource" || observed) return;
      const url = new URL(response.url);
      if (url.origin !== target.origin || url.pathname !== streamPath) return;
      observed = { status: response.status, protocol: response.protocol ?? null, mimeType: response.mimeType };
    });
    await page.goto(target.href);
    await expect.poll(() => observed, { timeout: STREAM_OPEN_TIMEOUT_MS }).toBeDefined();
    await test.info().attach("fleet-sse-protocol", {
      body: JSON.stringify({ hop: "browser-to-app", scheme: target.protocol, ...observed }),
      contentType: "application/json",
    });
    expect(observed?.status).toBe(200);
    expect(observed?.mimeType).toBe("text/event-stream");
    const multiplexed = ["h2", "h3"];
    const protocols = target.protocol === "https:" ? multiplexed : ["http/1.1", ...multiplexed];
    expect(protocols).toContain(observed?.protocol);
  } finally {
    await session.detach();
  }
}
