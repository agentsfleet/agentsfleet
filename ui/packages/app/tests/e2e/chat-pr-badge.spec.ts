import { expect, type Locator, type Page } from "@playwright/test";
import { test } from "./fixtures/chat-probe";
import { TASK_IDS, detail, summary } from "@/tests/helpers/task-reads";

test("late native PR result cards retain their association without moving a draft or visible reply", async ({page, chatProbe}) => {
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
      expect(reads()).toBe(2);
      const before = await readingGeometry(paragraph);
      ready.resolve();
      const badge = page.getByRole("link", {name: /^Pull request #123 ·/});
      await expect(badge).toBeVisible();
      expect(await badge.evaluate(node => node.closest('[aria-hidden="true"]'))).toBeNull();
      const after = await readingGeometry(paragraph);
      for (const [index, box] of before.boxes.entries()) {
        expect(box).not.toBeNull(); expect(after.boxes[index]).not.toBeNull();
        expect(Math.abs(box!.y - after.boxes[index]!.y)).toBeLessThanOrEqual(1);
        expect(Math.abs(box!.height - after.boxes[index]!.height)).toBeLessThanOrEqual(1);
      }
      const badgeBox = await badge.boundingBox();
      const identifier = badge.getByText("Pull request #123", {exact: true});
      const identifierBox = await identifier.boundingBox();
      expect(identifierBox).not.toBeNull();
      expect(identifierBox!.x + identifierBox!.width).toBeLessThanOrEqual(badgeBox!.x + badgeBox!.width + 1);
      expect(await identifier.evaluate(node => getComputedStyle(node).flexShrink)).toBe("0");
      expect(badgeBox!.y).toBeGreaterThanOrEqual(after.boxes[0]!.y + after.boxes[0]!.height);
      const replyBox = await paragraph.locator('xpath=ancestor::*[@data-role="assistant"]').boundingBox();
      expect(replyBox).not.toBeNull();
      expect(badgeBox!.x).toBeGreaterThanOrEqual(replyBox!.x - 1);
      expect(badgeBox!.x + badgeBox!.width).toBeLessThanOrEqual(replyBox!.x + replyBox!.width + 1);
      await expect(composer).toHaveValue(composing ? "Keep the callback checks intact." : "");
      // Inline phone Summary grows before the transcript in the same viewport.
      // Its independently measured growth must be compensated exactly; the
      // visible reply, header and composer retain their one-pixel bounds above.
      const viewportShift = after.origin - before.origin;
      const precedingGrowth = after.precedingSummaryHeight - before.precedingSummaryHeight;
      expect(Math.abs(after.scrollTop - before.scrollTop - viewportShift - precedingGrowth)).toBeLessThanOrEqual(1);
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

/** Read layout and its scroll offset in one browser turn, after the same layout flush. */
function readingGeometry(paragraph: Locator) {
  return paragraph.evaluate(node => {
    const root = node.closest('[data-testid="fleet-thread-root"]');
    const viewport = root?.querySelector<HTMLElement>('[role="presentation"]');
    const composer = viewport?.querySelector('textarea');
    const header = node.closest('[data-role="assistant"]')?.querySelector('[data-testid="chat-message-header"]');
    if (!viewport || !composer || !header) throw new Error("The visible chat geometry is unavailable");
    const boxes = [node, composer, header].map(element => {
      const {x, y, width, height} = element.getBoundingClientRect();
      return {x, y, width, height};
    });
    const origin = viewport.getBoundingClientRect().y;
    const summary = viewport.querySelector('[aria-label="Summary"]');
    const precedingSummaryHeight = summary && getComputedStyle(summary).order === "-1" ? summary.getBoundingClientRect().height : 0;
    return {boxes, origin, scrollTop: viewport.scrollTop, precedingSummaryHeight};
  });
}
