import {expect} from "@playwright/test";
import {test} from "./fixtures/chat-probe";

test("Models and Team use actual components at the approved scale", async ({page, chatProbe}, info) => {
  const errors: string[] = []; page.on("pageerror", error => errors.push(error.message));
  for (const theme of ["dark", "light"]) {
    await page.goto(`${chatProbe}?resources=1`);
    await page.evaluate(value => {document.documentElement.dataset.theme = value;}, theme);
    await expect(page.getByRole("heading", {name: "Models", exact: true})).toBeVisible();
    await page.getByRole("button", {name: "Create model", exact: true}).click();
    const dialog = page.getByRole("dialog", {name: "Create model", exact: true});
    await expect(dialog.getByRole("switch", {name: "Show advanced models"})).toHaveCount(0);
    await expect(dialog.getByRole("combobox", {name: "Model", exact: true})).toBeEnabled();
    await page.screenshot({path: info.outputPath(`create-model-default-${theme}.png`), scale: "css", animations: "disabled"});
    await dialog.getByRole("combobox", {name: "Provider", exact: true}).click();
    await page.getByRole("option", {name: "Fireworks", exact: true}).click();
    await dialog.getByRole("combobox", {name: "Model", exact: true}).click();
    await page.getByRole("option", {name: /GLM 5.2/}).click();
    await expect(dialog.getByText("Context cap (tokens)")).toBeHidden();
    const details = dialog.getByRole("button", {name: "Model details", exact: true});
    if (info.project.name === "chromium") {
      await details.hover();
      await expect(page.getByRole("tooltip")).toHaveText("Model details");
    }
    const picker = dialog.getByRole("combobox", {name: "Model", exact: true});
    const beforeDetails = await picker.boundingBox();
    await expect(details).toHaveAttribute("aria-expanded", "false");
    await details.click();
    await expect(details).toHaveAttribute("aria-expanded", "true");
    const detailsCard = page.getByRole("dialog", {name: "Model details", exact: true});
    await expect(detailsCard.getByText("Context cap (tokens)")).toBeVisible();
    expect(await detailsCard.evaluate(node => {
      const box = node.getBoundingClientRect();
      return box.width >= 280 && node.scrollWidth <= node.clientWidth + 1 &&
        box.left >= 0 && box.right <= innerWidth && box.top >= 0 && box.bottom <= innerHeight;
    })).toBe(true);
    expect(await picker.boundingBox()).toEqual(beforeDetails);
    await page.keyboard.press("Escape");
    await expect(detailsCard).toBeHidden();
    await expect(dialog).toBeVisible();
    await expect(details).toBeFocused();
    await expect(details).toHaveAttribute("aria-expanded", "false");
    await dialog.getByRole("radio", {name: "Use saved key"}).click();
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
    await expect(page.getByText("Email not sent · Share the invite link", {exact: true})).toBeVisible();
    expect(await page.getByText("Kishore Neelamegam", {exact: true}).evaluate(node => getComputedStyle(node).fontSize)).toBe("13px");
    expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
    await page.screenshot({path: info.outputPath(`team-${theme}.png`), scale: "css", animations: "disabled"});
    await page.getByRole("button", {name: "Invite member", exact: true}).click();
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
    const models = page.getByRole("region", {name: "Your models", exact: true});
    await expect(models.getByRole("listitem").first()).toBeVisible();
    const modelBounds = await models.boundingBox();
    expect(modelBounds?.width).toBeLessThanOrEqual(1040);
    expect(await models.getByRole("list").evaluate(node => getComputedStyle(node.parentElement!).borderRadius)).toBe("16px");
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
    await page.getByRole("button", {name: "Invite member"}).click();
    const invite = page.getByRole("dialog", {name: "Invite a teammate"});
    const email = invite.getByRole("textbox", {name: "Email address"});
    await expect(email).toBeFocused();
    const scope = invite.getByText(/They’ll join as a Member/).locator("..");
    const [fieldBox, scopeBox] = await Promise.all([email.boundingBox(), scope.boundingBox()]);
    expect(fieldBox?.x).toBe(scopeBox?.x);
    await expect(email).toHaveCSS("box-shadow", /inset/);
    await page.screenshot({path: info.outputPath(`invite-alignment-${theme}.png`), scale: "css", animations: "disabled"});
    await invite.getByRole("button", {name: "Cancel", exact: true}).click();
    await page.getByRole("region", {name: "Pending invitations", exact: true}).scrollIntoViewIfNeeded();
    const heading = page.getByRole("heading", {name: "Pending invitations 1", exact: true});
    expect(await heading.evaluate(element => {
      const count = element.lastElementChild;
      if (!count) return false;
      const row = element.getBoundingClientRect();
      const badge = count.getBoundingClientRect();
      return Math.abs(row.y + row.height / 2 - badge.y - badge.height / 2) < 1;
    })).toBe(true);
    await page.screenshot({path: info.outputPath(`matched-invitations-${theme}.png`), scale: "css", animations: "disabled"});
  }
});

test("remaining resource pages and New fleet retain the approved frame", async ({page, chatProbe}, info) => {
  const errors: string[] = [];
  page.on("pageerror", error => errors.push(error.message));
  await page.setViewportSize(info.project.name === "chromium" ? {width: 1480, height: 1000} : {width: 390, height: 844});
  for (const theme of ["dark", "light"]) {
    await page.goto(`${chatProbe}?resourcePages=1`);
    await page.evaluate(value => {document.documentElement.dataset.theme = value;}, theme);
    await page.evaluate(() => document.fonts.ready);
    for (const label of ["Usage", "Secrets", "Connected apps", "New fleet"]) {
      await page.locator("[data-review-pages]").getByRole("button", {name: label, exact: true}).click();
      if (label === "Usage") {
        const filter = page.getByRole("combobox", {name: "Period", exact: true});
        await expect(filter).toHaveCSS("font-size", "13px");
        if (info.project.name === "chromium") {
          const resting = await filter.evaluate(node => getComputedStyle(node).backgroundColor);
          await filter.hover();
          await expect(filter).not.toHaveCSS("background-color", resting);
        }
      }
      if (label === "New fleet") {
        const composer = page.getByRole("textbox", {name: "What would you like your fleet to do?"});
        await expect(page.getByRole("heading", {name: "New fleet"})).toBeVisible();
        await expect(composer).toBeVisible();
        await expect(page.getByRole("complementary", {name: "Summary"})).toHaveCount(0);
        const box = await composer.boundingBox();
        expect(box ? box.y + box.height : Infinity).toBeLessThanOrEqual(844 + (info.project.name === "chromium" ? 156 : 0));
      } else await expect(page.getByRole("heading", {level: 1})).toBeVisible();
      if (label === "Secrets") {
        const actions = page.getByRole("button", {name: "Manage Release service", exact: true});
        await expect(actions).toHaveCount(1);
        await actions.scrollIntoViewIfNeeded();
        await actions.focus();
        await expect(actions).toBeFocused();
        expect(await actions.evaluateAll(nodes => nodes.every(node => {
          const box = node.getBoundingClientRect();
          return box.left >= 0 && box.right <= innerWidth && box.width >= 32;
        }))).toBe(true);
        await actions.click();
        const management = page.getByRole("dialog", {name: "Manage secret", exact: true});
        await expect(management.getByText("“Release service”", {exact: true})).toBeVisible();
        await expect(management.getByText("••••••••", {exact: true})).toBeVisible();
        await expect(management.getByRole("button", {name: "Replace value"})).toBeVisible();
        await expect(management.getByRole("button", {name: "Delete secret"})).toBeVisible();
        await management.getByRole("button", {name: "Done", exact: true}).click();
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

test("the Secrets prototype lists sample secrets and adds a new entry", async ({page}) => {
  const errors: string[] = [];
  page.on("pageerror", error => errors.push(error.message));
  await page.goto(new URL("../../../../../docs/ux.html#secrets", import.meta.url).href);

  const secrets = page.getByRole("region", {name: "Stored secrets", exact: true});
  await expect(secrets.getByText("REGISTRY_TOKEN", {exact: true})).toBeVisible();
  await expect(secrets.getByText("SUPPORT_API_KEY", {exact: true})).toBeVisible();
  await secrets.getByRole("button", {name: "Manage REGISTRY_TOKEN", exact: true}).click();
  const manage = page.getByRole("dialog", {name: "Manage secret", exact: true});
  await expect(manage.getByText("Available to Release watch.", {exact: true})).toBeVisible();
  await manage.getByRole("button", {name: "Done", exact: true}).click();

  await page.getByRole("button", {name: "Add secret", exact: true}).click();
  const add = page.getByRole("dialog", {name: "Add secret", exact: true});
  await add.getByLabel(/^Name/).fill("SAMPLE_METRICS_KEY");
  await add.getByLabel("Value", {exact: true}).fill("preview-only");
  await add.getByRole("button", {name: "Add", exact: true}).click();
  await expect(add).toBeHidden();
  await expect(secrets.getByText("SAMPLE_METRICS_KEY", {exact: true})).toBeVisible();
  expect(errors).toEqual([]);
});

test("a connector refusal stays on its row at desktop and phone widths", async ({page, chatProbe}, info) => {
  for (const width of [1440, 390, 320]) {
    await page.setViewportSize({width, height: width === 1440 ? 960 : 844});
    for (const theme of ["dark", "light"]) {
      await page.goto(`${chatProbe}?resourcePages=1&resource=integrations&disconnectRoute=1`);
      await page.evaluate(value => {document.documentElement.dataset.theme = value;}, theme);
      await page.evaluate(() => document.fonts.ready);
      const github = page.getByTestId("integration-github");
      const slack = page.getByTestId("integration-slack");
      const disconnect = github.getByRole("button", {name: "Disconnect GitHub", exact: true});
      await expect(disconnect).toBeVisible();
      await expect(slack.getByRole("button", {name: "Connect Slack", exact: true})).toBeVisible();
      expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
      await page.screenshot({path: info.outputPath(`connected-apps-${width}-${theme}.png`), scale: "css", animations: "disabled"});
      const release = Promise.withResolvers<void>();
      await page.route("**/fixture/disconnect", async route => {
        await release.promise;
        await route.fulfill({status: 204});
      }, {times: 1});
      await disconnect.focus();
      try {
        await page.keyboard.press("Enter");
        const pending = github.getByRole("button", {name: "Disconnecting…", exact: true});
        await expect(pending).toBeDisabled();
        await expect(pending).toBeFocused();
      } finally {
        release.resolve();
      }
      await expect(github.getByRole("alert")).toBeVisible();
      await expect(github).toContainText("Connected · Release workspace");
      await expect(disconnect).toBeEnabled();
      await expect(disconnect).toBeFocused();
      await expect(page.getByRole("dialog")).toHaveCount(0);
    }
  }
});

test("the Connected apps prototype switches between Connect and Disconnect on the row", async ({page}) => {
  await page.goto(new URL("../../../../../docs/ux.html#connections", import.meta.url).href);
  const disconnect = page.getByRole("button", {name: "Disconnect Slack", exact: true});
  await expect(disconnect).toBeVisible();
  await expect(page.getByText("Invite the app to the Slack channels your fleets use.")).toBeVisible();
  await disconnect.focus();
  await page.keyboard.press("Enter");
  const connect = page.getByRole("button", {name: "Connect Slack", exact: true});
  await expect(connect).toBeFocused();
  await expect(page.getByText("Not connected", {exact: true})).toBeVisible();
  await expect(page.getByRole("button", {name: "Disconnect GitHub", exact: true})).toBeVisible();
  await expect(page.getByRole("dialog")).toHaveCount(0);
  await page.keyboard.press("Enter");
  await expect(disconnect).toBeFocused();
  await expect(page.getByText("Connected · agentsfleet", {exact: true})).toBeVisible();
});
