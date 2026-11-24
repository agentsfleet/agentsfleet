import {expect, type Locator} from "@playwright/test";

export async function expectSummaryColumns(panel: Locator, headings: Locator[], rows: Locator[]) {
  const title = await panel.getByRole("heading", {name: "Summary", exact: true}).boundingBox();
  if (!title) throw new Error("The Summary heading must be visible");
  const left = Math.round(title.x);
  for (const heading of headings) {
    const box = await heading.boundingBox();
    if (!box) throw new Error("A Summary section heading must be visible");
    expect(Math.round(box.x)).toBe(left);
  }
  for (const row of rows) {
    await expect(row).toBeVisible();
    const columns = await row.evaluate(element => {
      const [icon, label] = element.children;
      if (!icon?.matches("svg,i") || !label) throw new Error("A Summary row needs a leading icon and a label");
      const a = icon.getBoundingClientRect(), b = label.getBoundingClientRect();
      const repository = label.querySelector(".repo-name")?.getBoundingClientRect();
      return [a.left, a.width, b.left, (repository?.left ?? b.left) - b.left].map(Math.round);
    });
    expect(columns).toEqual([left, 16, left + 24, 0]);
  }
}
