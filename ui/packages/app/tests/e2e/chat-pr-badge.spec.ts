import { expect, type Page } from "@playwright/test";
import { test } from "./fixtures/chat-probe";
import { TASK_IDS, detail, summary } from "@/tests/helpers/task-reads";

test("late native PR badges stay on the sender line without moving a draft or visible reply", async ({page, chatProbe}) => {
  await page.emulateMedia({reducedMotion: "reduce"});
  for (const width of [1280, 390]) {
    await page.setViewportSize({width, height: 844});
    for (const composing of [true, false]) {
      const {ready, requested, reads} = await holdTaskDetail(page);
      await page.goto(`${chatProbe}?tasks=1`);
      const paragraph = page.getByText("The login repair is waiting for checks. I’m working on duplicate bookmarks next; the import repair is queued.", {exact: true});
      const composer = page.getByRole("textbox", {name: /^Message your fleet/});
      await expect(paragraph).toBeVisible();
      await composer.fill("Keep the callback checks intact.");
      if (!composing) {await composer.fill(""); await composer.blur();}
      await requested.promise;
      expect(await page.getByRole("link", {name: /^Pull request #123 ·/}).count()).toBe(0);
      const header = page.locator('[data-role="assistant"] [data-testid="chat-message-header"]');
      await expect(header).toHaveCount(1);
      const viewport = page.locator('[data-testid="fleet-thread-root"] [role="presentation"]');
      const readingPoint = await viewport.evaluate(node => node.scrollTop);
      expect(reads()).toBe(2);
      const before = await Promise.all([paragraph.boundingBox(), composer.boundingBox(), header.boundingBox()]);
      ready.resolve();
      const badge = page.getByRole("link", {name: /^Pull request #123 ·/});
      await expect(badge).toBeVisible();
      expect(await badge.evaluate(node => node.closest('[aria-hidden="true"]'))).toBeNull();
      const after = await Promise.all([paragraph.boundingBox(), composer.boundingBox(), header.boundingBox()]);
      for (const [index, box] of before.entries()) {
        expect(box).not.toBeNull(); expect(after[index]).not.toBeNull();
        expect(Math.abs(box!.y - after[index]!.y)).toBeLessThanOrEqual(1);
        expect(Math.abs(box!.height - after[index]!.height)).toBeLessThanOrEqual(1);
      }
      const badgeBox = await badge.boundingBox();
      expect(badgeBox!.y).toBeGreaterThanOrEqual(after[2]!.y);
      expect(badgeBox!.y + badgeBox!.height).toBeLessThanOrEqual(after[2]!.y + after[2]!.height + 1);
      await expect(composer).toHaveValue(composing ? "Keep the callback checks intact." : "");
      expect(Math.abs(await viewport.evaluate(node => node.scrollTop) - readingPoint)).toBeLessThanOrEqual(1);
      expect(reads()).toBe(2);
      if (composing) await expect(composer).toBeFocused();
      expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
      await page.unrouteAll({behavior: "wait"});
    }
  }
});

async function holdTaskDetail(page: Page) {
  const ready = Promise.withResolvers<void>();
  const requested = Promise.withResolvers<void>();
  let reads = 0;
  await page.route("**/live/v1/workspaces/workspace/fleets/linkwarden/tasks**", async route => {
    reads += 1;
    const pathname = new URL(route.request().url()).pathname;
    if (!pathname.endsWith(TASK_IDS[0])) {
      await route.fulfill({json: {items: [summary()], next_cursor: null}}); return;
    }
    requested.resolve();
    await ready.promise;
    const saved = detail();
    await route.fulfill({json: {...saved, context: {...saved.context, target: {kind: "existing_pr", repository: "linkwarden/linkwarden", number: 123,
      branch: "repair", head: "a".repeat(40), tree: "b".repeat(40), url: "https://github.com/linkwarden/linkwarden/pull/123", workflows: []}}}});
  });
  return {ready, requested, reads: () => reads};
}
