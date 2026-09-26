/**
 * fleet-thread.spec.ts — the operator's chat surface renders against the
 * durable event log for an authenticated user.
 *
 * This acceptance surface pins authenticated mount, workspace navigation,
 * the transcript/composer sibling layout, optimistic steer rendering, and
 * the rule that browser requests never carry a client Authorization header.
 * Stream frame handling and reconnect behaviour are covered by the focused
 * registry and event-stream tests where frame timing is deterministic.
 */
import type { Page } from "@playwright/test";
import { expect, test } from "@playwright/test";
import { signInAs } from "./fixtures/auth";
import { FIXTURE_KEY } from "./fixtures/constants";
import { getDefaultWorkspaceId, seedFleet, waitForFleetActive } from "./fixtures/seed";
import { cleanWorkspaceFleets } from "./fixtures/teardown";
import { workspaceHref, workspaceUrlPattern } from "./fixtures/nav";
import { SSE_CONTENT_TYPE, sseFrame } from "./fixtures/sse";
import { FRAME_KIND } from "@/lib/api/events-types";

const CHAT_LABEL = "Fleet chat";
const COMPOSER_LABEL = "Chat composer";

// One seed prefix per test, declared beside the name generators so the seeder
// and the afterEach sweep share a single literal and can never drift. The
// sweep is not optional hygiene: every seeded fleet carries a live cron
// trigger, so an unswept row keeps waking runners long after the run ends.
const THREAD_PREFIX = "thread-spec-";
const REVISIT_PREFIX = "thread-revisit-";
const STEER_PROBE_PREFIX = "steer-probe-";
const LAYOUT_PREFIX = "thread-layout-";
const SEED_PREFIXES = [THREAD_PREFIX, REVISIT_PREFIX, STEER_PROBE_PREFIX, LAYOUT_PREFIX] as const;

// A roomy window and one short enough that a tall draft outgrows the chat.
const LAYOUT_WINDOWS = [
  { width: 1280, height: 800 },
  { width: 1280, height: 360 },
] as const;
const TALL_DRAFT = Array.from({ length: 30 }, (_, line) => `draft line ${line + 1}`).join("\n");
// The textarea stops growing at 12rem or 30% of the window, whichever is
// lower (SteerComposer).
const DRAFT_MAX_PX = 192;
const DRAFT_WINDOW_SHARE = 0.3;
const LAYOUT_TOLERANCE_PX = 1;
// A reply long enough to overflow the chat, so the sticky composer rides over
// real history and the viewport, never a frame box, does the scrolling.
const LONG_REPLY = Array.from({ length: 80 }, (_, line) => `long streamed reply line ${line + 1}`).join("\n");
const LONG_REPLY_ID = "9100000000000-1";
const FUTURE_RUN_OFFSET_MS = 60_000;
// Below this much chat above the composer there is no room to show history.
const HISTORY_ROOM_PX = 48;

test.describe("fleet thread surface", () => {
  // Same shape as lifecycle.spec.ts: prefix-scoped so parallel workers
  // sharing the regular fixture workspace never delete a sibling's fleet.
  test.afterEach(async () => {
    const ws = await getDefaultWorkspaceId(FIXTURE_KEY.regular);
    for (const prefix of SEED_PREFIXES) {
      await cleanWorkspaceFleets(FIXTURE_KEY.regular, ws, prefix);
    }
  });

  test("renders the chat panel + composer for an authenticated user", async ({
    page,
  }) => {
    await signInAs(page, FIXTURE_KEY.regular);

    // Seed a uniquely-named fleet rather than reusing whatever an earlier
    // spec left behind: sibling specs clean the shared workspace from
    // parallel workers, and a borrowed fleet can vanish mid-test.
    const workspaceId = await getDefaultWorkspaceId(FIXTURE_KEY.regular);
    const tag = Math.random().toString(36).slice(2, 8);
    const fleet = await seedFleet(FIXTURE_KEY.regular, workspaceId, {
      name: `${THREAD_PREFIX}${tag}`,
    });
    await waitForFleetActive(FIXTURE_KEY.regular, workspaceId, fleet.id);

    await page.goto(workspaceHref(workspaceId, `fleets/${fleet.id}`));
    await expect(page).toHaveURL(workspaceUrlPattern(`fleets/${fleet.id}`));

    // Breadcrumb rendered server-side without duplicating the fleet name in a
    // second oversized title row.
    // Scope to the breadcrumb — the sidebar carries its own Fleets link, so
    // the bare role query became ambiguous when the breadcrumb shipped.
    await expect(
      page.getByLabel("Breadcrumb").getByRole("link", { name: "Fleets" }),
    ).toBeVisible();
    await expect(page.getByText(fleet.name, { exact: true }).first()).toBeVisible();

    // The thread card mounts client-side and consumes the shared stream registry.
    // Its accessible label is stable across visual changes.
    const threadCard = page.getByLabel(CHAT_LABEL);
    await expect(threadCard).toBeVisible({ timeout: 10_000 });

    // The selected Chat tab names this view; the transcript needs no second title.
    await expect(
      page.getByRole("navigation", { name: "Fleet sections" }).getByRole("link", { name: "Chat" }),
    ).toHaveAttribute("aria-current", "page");
    await expect(threadCard.getByRole("link", { name: "Steer" })).toHaveCount(0);

    // The conversation carries role="log" + aria-live=polite.
    const log = threadCard.getByRole("log", { name: /chat/i });
    await expect(log).toBeVisible();

    // The composer always renders and never disables itself — sending does
    // not depend on the live feed or on the fleet being idle.
    const composer = page.getByLabel(COMPOSER_LABEL);
    await expect(composer).toBeVisible();
    const placeholder = composer.getByPlaceholder(/message this fleet/i);
    await expect(placeholder).toBeVisible();
    await expect(placeholder).toBeEnabled();
  });

  test("keeps Send in view and the frame still over a long reply, roomy or short", async ({
    page,
  }) => {
    await signInAs(page, FIXTURE_KEY.regular);
    const workspaceId = await getDefaultWorkspaceId(FIXTURE_KEY.regular);
    const fleet = await seedFleet(FIXTURE_KEY.regular, workspaceId, {
      name: `${LAYOUT_PREFIX}${Math.random().toString(36).slice(2, 8)}`,
    });
    await waitForFleetActive(FIXTURE_KEY.regular, workspaceId, fleet.id);
    const streamPath = `/live/v1/workspaces/${workspaceId}/fleets/${fleet.id}/events/stream`;
    const release = Promise.withResolvers<void>();
    let streamRequests = 0;
    await page.route((url) => url.pathname === streamPath, async (route) => {
      streamRequests += 1;
      if (streamRequests > 1) {
        // Hold every reconnect so the run stays exactly as first delivered.
        await release.promise;
        await route.abort();
        return;
      }
      await route.fulfill({
        contentType: SSE_CONTENT_TYPE,
        body: [
          sseFrame(FRAME_KIND.EVENT_RECEIVED, {
            event_id: LONG_REPLY_ID, actor: "webhook:layout-probe", created_at: Date.now() + FUTURE_RUN_OFFSET_MS,
          }),
          sseFrame(FRAME_KIND.CHUNK, {
            event_id: LONG_REPLY_ID, text: LONG_REPLY, text_kind: "answer",
            stream_seq: 0, stream_start: true, stream_contiguous: true,
          }),
        ].join(""),
      });
    });
    try {
      await page.goto(workspaceHref(workspaceId, `fleets/${fleet.id}`));
      await expect(page.getByText("long streamed reply line 80")).toBeAttached();
      const draft = page.getByLabel(COMPOSER_LABEL).getByRole("textbox");

      for (const size of LAYOUT_WINDOWS) {
        // Blur first so the draft takes focus again at this size.
        await draft.blur();
        await page.setViewportSize(size);
        // fill() never presses Enter, so the draft is typed but never sent.
        await draft.fill(TALL_DRAFT);
        await afterPaint(page);
        const layout = await page.getByTestId("fleet-thread-root").evaluate((root) => {
          const edges = (el: Element) => {
            const { top, bottom, left, right } = el.getBoundingClientRect();
            return { top, bottom, left, right };
          };
          const scrolled: string[] = [];
          for (let el: Element | null = root; el; el = el.parentElement) {
            if (el.scrollTop !== 0) scrolled.push(`${el.tagName}#${el.id}.${el.className}`);
          }
          const messages = root.querySelectorAll('[data-testid="fleet-message"]');
          return {
            root: edges(root),
            footer: edges(root.querySelector('[data-testid="fleet-chat-footer"]')!),
            send: edges(root.querySelector('button[aria-label="Send"]')!),
            lastMessage: edges(messages[messages.length - 1]!),
            viewportScrollTop: root.querySelector('[role="presentation"]')!.scrollTop,
            draftHeight: root.querySelector("textarea")!.getBoundingClientRect().height,
            windowHeight: window.innerHeight,
            windowWidth: window.innerWidth,
            scrolled,
          };
        });
        await test.info().attach(`chat-layout-${size.height}`, {
          contentType: "application/json",
          body: JSON.stringify(layout),
        });
        // The composer sits on the chat floor; Send is inside the chat frame
        // and the window.
        expect(Math.abs(layout.footer.bottom - layout.root.bottom)).toBeLessThanOrEqual(LAYOUT_TOLERANCE_PX);
        expect(layout.send.top).toBeGreaterThanOrEqual(layout.root.top - LAYOUT_TOLERANCE_PX);
        expect(layout.send.bottom).toBeLessThanOrEqual(layout.root.bottom + LAYOUT_TOLERANCE_PX);
        expect(layout.send.right).toBeLessThanOrEqual(layout.root.right + LAYOUT_TOLERANCE_PX);
        expect(layout.root.bottom).toBeLessThanOrEqual(layout.windowHeight + LAYOUT_TOLERANCE_PX);
        expect(layout.root.right).toBeLessThanOrEqual(layout.windowWidth + LAYOUT_TOLERANCE_PX);
        expect(layout.draftHeight).toBeLessThanOrEqual(
          Math.min(DRAFT_MAX_PX, layout.windowHeight * DRAFT_WINDOW_SHARE) + LAYOUT_TOLERANCE_PX,
        );
        // Only the conversation viewport scrolls; the root and every frame box
        // above it stay put while the reply overflows and the draft grows.
        expect(layout.viewportScrollTop).toBeGreaterThan(0);
        expect(layout.scrolled).toEqual([]);
        // Where there is room, the newest reply stays above the composer.
        if (layout.footer.top - layout.root.top > HISTORY_ROOM_PX) {
          expect(layout.lastMessage.bottom).toBeLessThanOrEqual(layout.footer.top + LAYOUT_TOLERANCE_PX);
        }
      }
      await draft.fill("");
    } finally {
      release.resolve();
      await page.goto("about:blank");
      await page.unrouteAll({ behavior: "wait" });
    }
  });

  test("survives a /w/[workspaceId]/fleets ↔ /w/[workspaceId]/fleets/[id] round-trip without unmounting the surface", async ({
    page,
  }) => {
    // Pins the registry behavior end-to-end: navigating away and back
    // to the same fleet within the registry's idle window must NOT lose
    // the thread surface (a regression where the layout-level subscription
    // tears down on every nav would manifest as a CONNECTING flash on
    // every revisit, observable here as the badge value).
    await signInAs(page, FIXTURE_KEY.regular);
    const workspaceId = await getDefaultWorkspaceId(FIXTURE_KEY.regular);
    const tag = Math.random().toString(36).slice(2, 8);
    const fleet = await seedFleet(FIXTURE_KEY.regular, workspaceId, {
      name: `${REVISIT_PREFIX}${tag}`,
    });
    await waitForFleetActive(FIXTURE_KEY.regular, workspaceId, fleet.id);

    await page.goto(workspaceHref(workspaceId, `fleets/${fleet.id}`));
    await expect(page.getByLabel(CHAT_LABEL)).toBeVisible({
      timeout: 10_000,
    });

    await page.goto(workspaceHref(workspaceId, "fleets"));
    await expect(page).toHaveURL(workspaceUrlPattern("fleets"));

    // Return. The thread surface must re-render; behavior parity with the
    // first mount is the assertion — we don't claim "no reconnect" at the
    // network layer from a Playwright test (that's the registry unit-test
    // surface), only that the user-visible surface comes back cleanly.
    await page.goto(workspaceHref(workspaceId, `fleets/${fleet.id}`));
    await expect(page.getByLabel(CHAT_LABEL)).toBeVisible({
      timeout: 10_000,
    });
    await expect(
      page.getByLabel(COMPOSER_LABEL),
    ).toBeVisible();
  });

  test("steer submits via a Server Action and no same-origin request carries a client Authorization header", async ({
    page,
  }) => {
    // Dimension 1.1 — the security invariant of this milestone. Steering
    // rides a Server Action (POST with a `Next-Action` header), not a
    // client fetch to /backend; and no same-origin request (the page
    // route, the /backend SSE proxy, any app fetch) ever carries a
    // browser-set bearer token. The SSE route handler injects the token
    // server-side, so its request is cookie-only here too.
    await signInAs(page, FIXTURE_KEY.regular);
    const workspaceId = await getDefaultWorkspaceId(FIXTURE_KEY.regular);
    const tag = Math.random().toString(36).slice(2, 8);
    const fleet = await seedFleet(FIXTURE_KEY.regular, workspaceId, {
      name: `${STEER_PROBE_PREFIX}${tag}`,
    });
    await waitForFleetActive(FIXTURE_KEY.regular, workspaceId, fleet.id);

    const seen: { method: string; url: string; auth: boolean; serverAction: boolean }[] = [];
    page.on("request", (req) => {
      const h = req.headers();
      seen.push({
        method: req.method(),
        url: req.url(),
        auth: Boolean(h["authorization"]),
        serverAction: Boolean(h["next-action"]),
      });
    });

    await page.goto(workspaceHref(workspaceId, `fleets/${fleet.id}`));
    const appOrigin = new URL(page.url()).origin;
    const threadCard = page.getByLabel(CHAT_LABEL);
    await expect(threadCard).toBeVisible({ timeout: 10_000 });

    const composer = page.getByLabel(COMPOSER_LABEL);
    const textarea = composer.getByPlaceholder(/message this fleet/i);
    await expect(textarea).toBeVisible();
    await textarea.fill("acceptance steer probe");
    await composer.getByRole("button", { name: /send/i }).click();

    // The optimistic row renders the message text immediately, regardless
    // of whether the send ultimately resolves to sent or to failed.
    await expect(threadCard.getByText(/acceptance steer probe/)).toBeVisible({
      timeout: 5_000,
    });

    // A Server Action POST carried the steer (not a client /backend fetch).
    await expect
      .poll(() => seen.filter((r) => r.method === "POST" && r.serverAction).length, {
        timeout: 10_000,
      })
      .toBeGreaterThan(0);

    // Load-bearing assertion: zero same-origin requests carried a
    // browser-set Authorization header.
    const authHits = seen
      .filter((r) => r.url.startsWith(appOrigin) && r.auth)
      .map((r) => `${r.method} ${r.url}`);
    expect(authHits, authHits.join("\n")).toEqual([]);
  });
});

async function afterPaint(page: Page): Promise<void> {
  await page.evaluate(() => new Promise<void>((resolve) => {
    requestAnimationFrame(() => requestAnimationFrame(() => resolve()));
  }));
}
