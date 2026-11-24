import {expect, test} from "@playwright/test";

const review = (view: string) => new URL(`../../../../../docs/ux.html#${view}`, import.meta.url).href;

test("the populated navigation keeps edit targets inside the sidebar and credits in the footer", async ({page, isMobile}) => {
  test.skip(isMobile, "The mobile sidebar uses an off-canvas fleet list");
  await page.setViewportSize({width: 1120, height: 660});
  await page.goto(review("chief"));
  const nav = page.getByRole("navigation", {name: "Main navigation"});
  await expect(nav.getByRole("button", {name: "Edit Chief of Staff"})).toBeVisible();
  await expect(nav.getByRole("button", {name: /Credits.*\$5\.00/})).toHaveCSS("border-top-width", "0px");
  await expect(page.getByRole("link", {name: /tightening review/i})).toHaveCount(0);
  await expect(page.getByRole("button", {name: "Model proposal"})).toHaveCount(0);
  expect(await page.locator(".review-links button[data-view]").evaluateAll(buttons =>
    buttons.slice(0, 3).map(button => button.textContent))).toEqual(["App loading", "New fleet", "Fleet setup"]);
  const geometry = await nav.evaluate(rail => {
    const {right, bottom} = rail.getBoundingClientRect();
    const footer = rail.querySelector(".rail-bottom")!.getBoundingClientRect();
    const edges = Array.from(rail.querySelectorAll(".fleet-edit"), edit => edit.getBoundingClientRect().right);
    return {right, bottom, footerBottom: footer.bottom, edges};
  });
  expect(geometry.edges.length).toBeGreaterThan(4);
  expect(new Set(geometry.edges).size).toBe(1);
  expect(geometry.edges[0]).toBeLessThan(geometry.right);
  expect(geometry.footerBottom).toBeLessThan(geometry.bottom);
});

test("new fleet holds its composer position while unfinished setup history resolves", async ({page}) => {
  await page.emulateMedia({reducedMotion: "reduce"});
  await page.goto(review("empty"));
  await page.evaluate(() => document.fonts.ready);
  const scenario = page.getByRole("combobox", {name: "New fleet workspace example"});
  const composer = page.locator(".empty-workspace .composer");
  const drafts = page.getByRole("region", {name: "Unfinished setups"});
  await scenario.selectOption("fresh");
  await expect(drafts).toBeHidden();
  const before = await composer.boundingBox();
  await scenario.selectOption("unfinished");
  await expect(drafts).toBeVisible();
  expect(await composer.boundingBox()).toEqual(before);
  await drafts.getByRole("button", {name: "Deployment checks"}).click();
  await expect(page.locator("#page-title")).toHaveText("Deployment checks");
  await expect(drafts).toHaveCount(0);
});

test("clicking the title edits it in place and blur saves it", async ({page}) => {
  await page.goto(review("draft"));
  const title = page.locator("#page-title");
  const input = page.getByRole("textbox", {name: "Chat name"});
  const titleBox = await title.boundingBox();
  await title.click();
  await expect(input).toBeFocused();
  await expect(input).toHaveCSS("background-color", "rgba(0, 0, 0, 0)");
  await expect(input).toHaveCSS("border-top-width", "0px");
  const editBox = await input.boundingBox();
  expect(editBox!.width).toBeLessThan(titleBox!.width + 8);
  expect(editBox!.height).toBeCloseTo(titleBox!.height, 0);
  await expect(page.getByRole("dialog")).toHaveCount(0);
  await input.fill("Release checklist");
  await page.locator(".user-bubble").first().click();
  await expect(title).toHaveText("Release checklist");
  await expect(input).toBeHidden();
  await expect(title).toHaveAttribute("data-name-saved", "");

  await title.press("Enter");
  await input.fill("Discard this name");
  await input.press("Escape");
  await expect(title).toHaveText("Release checklist");
  await expect(title).toBeFocused();
});

test("model creation keeps the provider and model usable while saved keys load", async ({page}) => {
  await page.goto(review("model-loading"));
  const dialog = page.getByRole("dialog", {name: "Create model"});
  await expect(dialog.getByText("Checking saved keys…")).toBeVisible();
  await expect(dialog.getByRole("combobox", {name: "Provider"})).toBeEnabled();
  await expect(dialog.getByRole("combobox", {name: "Model", exact: true})).toBeEnabled();
  await expect(dialog.getByRole("button", {name: "Create model"})).toBeDisabled();
  await dialog.getByRole("radio", {name: "Add new key"}).check();
  await expect(dialog.getByText("Checking saved keys…")).toBeHidden();
  await expect(dialog.locator("#model-api-key")).toBeEditable();
  await page.keyboard.press("Escape");
  await expect(page.getByRole("button", {name: "Add another model"})).toHaveCount(0);
});

test("Create model opens catalogue details within the model form", async ({page}) => {
  await page.goto(review("models"));
  await page.getByRole("button", {name: "Create model", exact: true}).click();
  const dialog = page.getByRole("dialog", {name: "Create model"});
  await dialog.getByRole("combobox", {name: "Provider"}).selectOption("anthropic");
  await dialog.getByRole("combobox", {name: "Model", exact: true}).selectOption("Fable 5.1");
  await dialog.getByRole("button", {name: "Model details"}).click();
  await expect(dialog.getByText("Price schedule (1)", {exact: true})).toBeVisible();
});

test("the conversation has one retry action and reports when the check succeeds", async ({page}) => {
  const time = new Date("2026-11-23T00:00:00Z");
  await page.clock.install({time});
  await page.clock.pauseAt(new Date(time.valueOf() + 1_000));
  await page.goto(review("failure"));
  await expect(page.locator(".error-card")).toHaveCount(0);
  await page.getByRole("button", {name: "Reconnect & retry"}).click();
  await expect(page.getByRole("button", {name: "Retrying…"})).toBeDisabled();
  await page.clock.fastForward(700);
  await expect(page.getByText("Reconnected. Release v2.8.1 is healthy.", {exact: true}).first()).toBeVisible();
  await expect(page.getByRole("button", {name: "Reconnect & retry"})).toHaveCount(0);
});

test("the CLI tabs show sign-in progress and a delayed runner request", async ({page}) => {
  const time = new Date("2026-11-23T00:00:00Z");
  await page.clock.install({time});
  await page.clock.pauseAt(new Date(time.valueOf() + 1_000));
  await page.goto(review("cli-sign-in"));
  await page.getByRole("button", {name: "Authorize", exact: true}).click();
  await expect(page.locator("[data-action=authorize-cli]")).toBeDisabled();
  await expect(page.getByRole("status").filter({hasText: "Authorizing"})).toBeVisible();
  await page.clock.fastForward(900);
  await expect(page.getByRole("heading", {name: "Authorization granted"})).toBeVisible();
  await page.goto(review("terminal"));
  await page.getByRole("button", {name: "Replay a slow request"}).click();
  await page.clock.fastForward(400);
  await expect(page.getByRole("status").filter({hasText: "Waiting for agentsfleet"})).toBeVisible();
  await expect(page.locator(".mini-orbit")).toBeVisible();
  await page.clock.fastForward(1_050);
  await expect(page.locator("#runner-list-output")).toBeVisible();
  await expect(page.getByRole("status").filter({hasText: "Waiting for agentsfleet"})).toBeHidden();
  const color = (state: string) => page.locator(`#runner-list-output .${state}`).first().evaluate(node => getComputedStyle(node).color);
  expect(new Set(await Promise.all(["active", "online", "offline"].map(color))).size).toBe(3);
});
