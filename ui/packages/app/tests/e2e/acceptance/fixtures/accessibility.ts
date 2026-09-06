import type { Page } from "@playwright/test";
import { expect } from "@playwright/test";
import AxeBuilder from "@axe-core/playwright";

export async function expectAccessible(page: Page, state: string): Promise<void> {
  await page.evaluate(async () => {
    await document.fonts.ready;
    const transitions = document.getAnimations().filter(animation =>
      animation.effect?.getTiming().iterations !== Infinity);
    await Promise.all(transitions.map(animation => animation.finished.catch(() => {})));
  });
  const results = await new AxeBuilder({ page }).analyze();
  // Report selectors and rule explanations, never HTML containing account data.
  const violations = results.violations.map(violation => ({
    rule: violation.id,
    nodes: violation.nodes.map(node => ({ target: node.target, reason: node.failureSummary })),
  }));
  expect.soft(violations, state).toEqual([]);
}

export async function expectNoPageOverflow(page: Page): Promise<void> {
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
}
