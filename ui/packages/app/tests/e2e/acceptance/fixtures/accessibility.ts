import type { Page } from "@playwright/test";
import { expect } from "@playwright/test";
import AxeBuilder from "@axe-core/playwright";

// Live rows may mount after the animation snapshot. Override only the
// entrance keyframe's opacity so both motion modes keep their own colors.
const ENTRANCE_OPACITY_CSS = `
  :is([class~="animate-in"], [class~="motion-safe:animate-in"]) {
    --tw-enter-opacity: 1 !important;
  }
`;

export async function expectAccessible(page: Page, state: string): Promise<void> {
  const style = await page.addStyleTag({ content: ENTRANCE_OPACITY_CSS });
  try {
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
  } finally {
    await style.evaluate(element => element.remove());
  }
}

export async function expectNoPageOverflow(page: Page): Promise<void> {
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
}
