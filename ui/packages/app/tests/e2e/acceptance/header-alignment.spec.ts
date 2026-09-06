import { expect, test } from "@playwright/test";
import { signInAs } from "./fixtures/auth";
import { expectAccessible, expectNoPageOverflow } from "./fixtures/accessibility";

for (const width of [1440, 390, 320]) {
  test(`runner actions and metadata use consistent sizes at ${width}px`, async ({ page }) => {
    await page.setViewportSize({ width, height: 900 });
    await signInAs(page, "operator");
    await page.goto("/admin/runners");
    await page.locator('a[href^="/admin/runners/"]').first().click();
    const edit = page.getByRole("button", { name: "Edit policy", exact: true });
    const refresh = page.getByRole("button", { name: "Refresh", exact: true });
    await expect(edit).toBeVisible();
    await expect(refresh).toBeVisible();
    const editBox = await edit.boundingBox();
    const refreshBox = await refresh.boundingBox();
    expect(Math.abs(editBox!.height - refreshBox!.height)).toBeLessThanOrEqual(1);
    const labels = page.getByTestId("runner-labels").locator(":scope > div");
    const sizes = await labels.evaluateAll(elements => elements.map(element => {
      const style = getComputedStyle(element);
      return { height: element.getBoundingClientRect().height, font: style.fontSize, line: style.lineHeight };
    }));
    expect(sizes.length).toBeGreaterThan(0);
    for (const size of sizes) expect(size).toEqual(sizes[0]);
    await expectNoPageOverflow(page);
    await expectAccessible(page, "Runner header and detail");
  });
}
