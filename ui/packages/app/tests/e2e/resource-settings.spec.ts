import {expect} from "@playwright/test";
import {test} from "./fixtures/chat-probe";

test("Models and Team use actual components at the approved scale", async ({page, chatProbe}, info) => {
  const errors: string[] = []; page.on("pageerror", error => errors.push(error.message));
  for (const theme of ["dark", "light"]) {
    await page.goto(`${chatProbe}?resources=1`);
    await page.evaluate(value => {document.documentElement.dataset.theme = value;}, theme);
    await expect(page.getByRole("heading", {name: "Models", exact: true})).toBeVisible();
    await page.getByRole("button", {name: "Create model", exact: true}).click();
    const dialog = page.getByRole("dialog");
    await dialog.getByRole("combobox", {name: "Provider", exact: true}).click();
    await page.getByRole("option", {name: "Fireworks", exact: true}).click();
    await dialog.getByRole("combobox", {name: "Model", exact: true}).click();
    await page.getByRole("option", {name: /GLM 5.2/}).click();
    await expect(dialog.getByText("Context cap (tokens)")).toBeHidden();
    await dialog.locator("summary").filter({hasText: "Model details"}).click();
    await expect(dialog.getByText("Context cap (tokens)")).toBeVisible();
    await dialog.locator("summary").filter({hasText: "Model details"}).click();
    await dialog.getByRole("button", {name: "Use saved key"}).click();
    await dialog.getByRole("combobox", {name: "Saved key"}).click();
    await page.getByRole("option", {name: "Fireworks workspace"}).click();
    await expect(dialog.getByRole("combobox", {name: "Saved key"})).toHaveText("Fireworks workspace");
    expect(await dialog.getByRole("button", {name: "Create model", exact: true}).evaluate(node => getComputedStyle(node).fontSize)).toBe("13px");
    expect(await dialog.evaluate(node => node.getBoundingClientRect().right <= innerWidth)).toBe(true);
    expect(await dialog.getByRole("button", {name: "Create model", exact: true}).evaluate(node => node.getBoundingClientRect().bottom <= innerHeight)).toBe(true);
    await page.screenshot({path: info.outputPath(`create-model-${theme}.png`), scale: "css", animations: "disabled"});
    await dialog.getByRole("button", {name: "Cancel", exact: true}).click();
    await page.getByRole("button", {name: "Team", exact: true}).click();
    await expect(page.getByRole("heading", {name: "Team", exact: true})).toBeVisible();
    await expect(page.getByText("Email not sent", {exact: true}).filter({visible: true})).toBeVisible();
    expect(await page.getByText("Kishore Neelamegam", {exact: true}).evaluate(node => getComputedStyle(node).fontSize)).toBe("13px");
    expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
    await page.screenshot({path: info.outputPath(`team-${theme}.png`), scale: "css", animations: "disabled"});
    await page.getByRole("button", {name: "Invite", exact: true}).click();
    await expect(page.getByRole("dialog")).toBeVisible();
    await page.screenshot({path: info.outputPath(`invite-${theme}.png`), scale: "css", animations: "disabled"});
  }
  expect(errors).toEqual([]);
});

test("resource cards match the approved viewport and retain readable phone content", async ({page, chatProbe}, info) => {
  await page.setViewportSize(info.project.name === "chromium" ? {width: 1480, height: 1000} : {width: 390, height: 844});
  for (const theme of ["dark", "light"]) {
    await page.goto(`${chatProbe}?resources=1`);
    await page.evaluate(value => {document.documentElement.dataset.theme = value;}, theme);
    await page.evaluate(() => document.fonts.ready);
    const models = page.getByRole("region", {name: "Models", exact: true});
    await expect(models.getByRole("article").first()).toBeVisible();
    const modelBounds = await models.boundingBox();
    expect(modelBounds?.width).toBeLessThanOrEqual(1040);
    expect(await models.getByRole("article").first().evaluate(node => getComputedStyle(node).borderRadius)).toBe("16px");
    await page.screenshot({path: info.outputPath(`matched-models-${theme}.png`), scale: "css", animations: "disabled"});
    await page.getByRole("button", {name: "Team", exact: true}).click();
    await expect(page.getByRole("heading", {name: "Team", exact: true})).toBeVisible();
    const people = page.getByRole("region", {name: "People", exact: true});
    const cards = people.getByRole("listitem");
    await expect(cards).toHaveCount(3);
    expect(await cards.evaluateAll(nodes => nodes.every(node => {
      const box = node.getBoundingClientRect();
      return box.left >= 0 && box.right <= innerWidth && node.scrollWidth <= node.clientWidth + 1;
    }))).toBe(true);
    await page.screenshot({path: info.outputPath(`matched-team-${theme}.png`), scale: "css", animations: "disabled"});
    await page.getByRole("region", {name: "Pending invitations", exact: true}).scrollIntoViewIfNeeded();
    await page.screenshot({path: info.outputPath(`matched-invitations-${theme}.png`), scale: "css", animations: "disabled"});
  }
});

test("remaining resource pages and empty setup retain the approved frame", async ({page, chatProbe}, info) => {
  const errors: string[] = [];
  page.on("pageerror", error => errors.push(error.message));
  await page.setViewportSize(info.project.name === "chromium" ? {width: 1480, height: 1000} : {width: 390, height: 844});
  await page.route("**/live/v1/workspaces/*/fleet-libraries?*", route => route.fulfill({json: {items: [{
    id: "release-watch", name: "Release watch", description: "A saved setup for checking image builds and release readiness.",
    visibility: "tenant", head_revision_id: "01924f4e-0000-7000-8000-00000000c4a1", etag: "sample", requirements: {},
    created_at: "2026-11-11T10:00:00Z", required_credentials_reasons: {},
  }], next_cursor: null}}));
  for (const theme of ["dark", "light"]) {
    await page.goto(`${chatProbe}?resourcePages=1`);
    await page.evaluate(value => {document.documentElement.dataset.theme = value;}, theme);
    await page.evaluate(() => document.fonts.ready);
    for (const label of ["Usage", "Secrets", "Plugins", "Connected apps", "API keys", "New fleet"]) {
      await page.locator("[data-review-pages]").getByRole("button", {name: label, exact: true}).click();
      if (label === "Plugins") await expect(page.getByText("Release watch", {exact: true})).toBeVisible();
      else if (label === "New fleet") {
        const composer = page.getByRole("textbox", {name: "Describe the work you want this fleet to do…"});
        await expect(page.getByRole("heading", {name: "What should your fleet do?"})).toBeVisible();
        await expect(composer).toBeVisible();
        await expect(page.getByRole("complementary", {name: "Summary"})).toHaveCount(0);
        const box = await composer.boundingBox();
        expect(box ? box.y + box.height : Infinity).toBeLessThanOrEqual(844 + (info.project.name === "chromium" ? 156 : 0));
      } else await expect(page.getByRole("heading", {level: 1})).toBeVisible();
      if (label === "Secrets" || label === "API keys") {
        const actions = label === "Secrets" ? page.getByRole("button", {name: /^(Edit|Delete) secret Release service/}) : page.getByRole("button", {name: "Revoke API key Release tooling"});
        await expect(actions).toHaveCount(label === "Secrets" ? 2 : 1);
        expect(await actions.evaluateAll(nodes => nodes.every(node => {
          const box = node.getBoundingClientRect();
          return box.left >= 0 && box.right <= innerWidth && box.width >= 32;
        }))).toBe(true);
        const previous = page.getByRole("button", {name: "Previous page"});
        const next = page.getByRole("button", {name: "Next page"});
        if (await previous.count()) expect((await previous.boundingBox())?.y).toBe((await next.boundingBox())?.y);
      }
      expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
      const heading = page.getByRole("heading", {level: 1});
      expect(await heading.evaluate(node => getComputedStyle(node).fontFamily)).toContain("Manrope");
      await page.screenshot({path: info.outputPath(`matched-${label.toLowerCase().replaceAll(" ", "-")}-${theme}.png`), scale: "css", animations: "disabled"});
    }
  }
  expect(errors).toEqual([]);
});
