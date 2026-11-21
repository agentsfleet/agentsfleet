import {expect, type Locator, type Page, type TestInfo} from "@playwright/test";
import {test} from "./fixtures/chat-probe";

const text = ["font-family", "font-size", "line-height", "font-weight", "color"];
const panel = ["width", "border-radius", "border-color", "background-color", "padding-left", "padding-top"];
const icon = ["width", "height", "color"];
const field = [...text, "width", "height", "border-radius", "background-color"];

for (const view of ["Models", "Team"] as const) {
  test(`${view} matches the reference at both palettes and supports its keyboard workflows`, async ({page, chatProbe, isMobile}, info) => {
    const reference = await page.context().newPage();
    const viewport = isMobile ? {width: 390, height: 844} : {width: 1440, height: 1000};
    for (const surface of [page, reference]) {
      await surface.setViewportSize(viewport);
      await surface.emulateMedia({reducedMotion: "reduce"});
    }
    await Promise.all([
      page.goto(`${chatProbe}?resources=1&parity=1&visual=1`),
      reference.goto(new URL(`../../../../../docs/ux.html#${view.toLowerCase()}`, import.meta.url).href),
    ]);
    if (view === "Team") await page.getByRole("button", {name: "Team", exact: true}).click();
    const title = page.getByRole("heading", {level: 1, name: view});
    const designTitle = reference.locator(".page-heading h2");
    const trigger = page.getByRole("button", {name: view === "Models" ? "Create model" : "Invite member", exact: true});
    const designTrigger = reference.locator(".page-heading .primary");

    for (const palette of ["dark", "light"]) {
      for (const surface of [page, reference]) {
        await surface.evaluate(async theme => {
          document.documentElement.dataset.theme = theme;
          await document.fonts.ready;
        }, palette);
      }
      await match(title, designTitle, text);
      await match(title.locator("..").locator("p"), reference.locator(".page-heading p"), text);
      await match(trigger, designTrigger, [...text, "width", "height", "gap", "border-radius", "background-color"]);
      await match(trigger.locator("svg"), designTrigger.locator("i"), icon);
      if (view === "Models") await reviewModels(page, reference);
      else await reviewTeam(page, reference, isMobile);
      await capture(page, reference, info, view.toLowerCase(), palette);

      if (isMobile) {
        const appAnchor = view === "Models" ? page.getByRole("button", {name: "Add another model"}) : page.getByRole("listitem", {name: "sofia.chen@example.com"});
        const refAnchor = reference.locator(view === "Models" ? ".ask-strip button" : ".invite-row:last-child");
        for (const anchor of [appAnchor, refAnchor]) await anchor.evaluate(element => element.scrollIntoView({block: "end"}));
        await capture(page, reference, info, `${view.toLowerCase()}-lower`, palette);
        for (const anchor of [title, designTitle]) await anchor.evaluate(element => {
          for (let parent = element.parentElement; parent; parent = parent.parentElement) parent.scrollTop = 0;
        });
      }
      await trigger.press("Enter");
      await designTrigger.click();
      const dialog = page.getByRole("dialog"), design = reference.getByRole("dialog");
      await match(dialog, design, panel);
      await match(dialog.getByRole("heading"), design.locator(".modal-heading h2"), ["font-family", "font-size", "font-weight", "color"]);
      await match(dialog.locator("[data-dialog-close] svg"), design.locator(".modal-heading button i"), icon);
      await match(dialog.getByRole("button", {name: "Cancel"}), design.locator(".modal-actions .secondary"), [...text, "height", "width", "border-radius"]);
      const refInput = design.locator(view === "Models" ? "#new-model-provider" : "#team-invite-email");
      const input = view === "Models" ? dialog.getByRole("combobox", {name: "Provider", exact: true}) : dialog.getByRole("textbox", {name: "Email address"});
      if (view === "Models") {
        // The reference opens with OpenAI selected. Production asks the owner to choose.
        await input.press("Enter");
        await page.getByRole("option", {name: "OpenAI", exact: true}).press("Enter");
      }
      await match(input, refInput, field);
      expect(await topOffset(dialog, input)).toBeCloseTo(await topOffset(design, refInput), 0);
      if (view === "Models") {
        await expect(dialog.getByLabel("Key name", {exact: true})).toBeVisible();
        await match(dialog.getByRole("radiogroup"), design.locator(".credential-tabs"), ["width", "height", "background-color", "border-color", "border-radius", "gap"]);
        await match(dialog.getByLabel("Key name", {exact: true}), design.locator("#model-key-name"), field);
      } else {
        await expect(input).toBeFocused();
        await expect(input).toHaveAccessibleDescription(/every workspace in this account/);
        await expect(dialog.getByText("Kishore’s account", {exact: true})).toBeVisible();
        const scope = dialog.locator("p").filter({hasText: "They’ll join as a"});
        await match(scope, design.locator(".team-invite-scope"), panel);
        await match(scope.locator("svg"), design.locator(".team-invite-scope i"), icon);
        expect(await topOffset(input, scope)).toBe(await topOffset(refInput, design.locator(".team-invite-scope")));
      }
      await capture(page, reference, info, `${view.toLowerCase()}-dialog`, palette);
      await dialog.getByRole("button", {name: "Close", exact: true}).focus();
      await page.keyboard.press("Tab");
      expect(await dialog.evaluate(element => element.contains(document.activeElement))).toBe(true);
      await page.keyboard.press("Escape");
      await expect(trigger).toBeFocused();
      await reference.keyboard.press("Escape");
      await expect(design).toBeHidden();
    }

    if (view === "Team") await useTeam(page);
    else await useModels(page);
    await reference.close();
  });
}

async function reviewModels(page: Page, reference: Page) {
  const card = page.getByLabel("Platform default", {exact: true});
  const design = reference.locator(".models-default");
  await match(card, design, panel);
  await match(card.locator("svg"), design.locator(".resource-icon i"), icon);
  await match(card.locator("h2"), design.locator("strong"), text);
  await match(card.locator("p"), design.locator("p"), text);
  await match(card.getByText("Active", {exact: true}), design.locator(".model-active"), [...text, "gap"]);
  const section = page.getByRole("region", {name: "Your models"});
  const heading = section.getByRole("heading");
  const designHeading = reference.locator(".section-title h3");
  const count = heading.getByText("1"), designCount = designHeading.locator(".label-chip");
  await match(heading, designHeading, [...text, "height"]);
  await match(count, designCount, [...text, "width", "height", "border-radius"]);
  expect(await topOffset(heading, count)).toBe(await topOffset(designHeading, designCount));
  await match(section.getByRole("listitem"), reference.locator(".model-row"), ["height", "width", "padding-left", "padding-top", "gap"]);
  await match(section.getByText("OpenAI", {exact: true}), reference.locator(".resource-copy p"), text);
  await match(section.locator("p svg"), reference.locator(".model-row small i"), icon);
}

async function reviewTeam(page: Page, reference: Page, isMobile: boolean) {
  const scope = page.getByText("Kishore’s account", {exact: true}).locator("../..");
  await match(scope, reference.locator(".team-scope"), ["width", "height", "padding-bottom", "gap", "border-bottom-color"]);
  await match(scope.locator("svg"), reference.locator(".team-scope-icon i"), icon);
  const search = page.getByRole("searchbox");
  await match(search, reference.locator("#team-search"), text);
  await match(search.locator("..").locator("svg"), reference.locator(".search i"), icon);
  const row = page.getByRole("listitem", {name: "Kishore", exact: true});
  await match(row, reference.locator(".team-row").first(), ["height", "width", "padding-left", "padding-top"]);
  await match(row.getByText("kishore@example.com"), reference.locator(".team-person > span").first(), text);
  await match(page.getByText("Email not sent · Share the invite link"), reference.locator(".team-warning"), text);
  const avatar = row.locator(":scope > [aria-hidden]").first();
  expect(await avatar.isVisible()).toBe(!isMobile);
  if (!isMobile) await match(avatar, reference.locator(".team-avatar").first(), [...text, "width", "height", "background-color"]);
}

async function useTeam(page: Page) {
  const search = page.getByRole("searchbox");
  await search.fill("maya");
  await expect(page.getByRole("heading", {name: "Members 2"})).toBeVisible();
  await expect(page.getByText("No invitations match your search.")).toBeVisible();
  const manage = page.getByRole("button", {name: "Manage Maya Rao"});
  await manage.press("Enter");
  await expect(page.getByRole("menuitem", {name: "Remove Maya Rao"})).toBeFocused();
  await page.keyboard.press("Escape");
  await expect(manage).toBeFocused();
  await search.fill("absent");
  await expect(page.getByText("No members match your search.")).toBeVisible();
  await page.getByRole("button", {name: "Clear search"}).press("Enter");
  await expect(page.getByRole("listitem", {name: "leon.park@example.com"})).toBeVisible();
}

async function useModels(page: Page) {
  const manage = page.getByRole("button", {name: "Manage gpt-4.1"});
  await manage.press("Enter");
  await expect(page.getByRole("menuitem", {name: "Model details"})).toBeFocused();
  await page.keyboard.press("Escape");
  await expect(manage).toBeFocused();
  await page.getByRole("button", {name: "Add another model"}).press("Enter");
  const dialog = page.getByRole("dialog");
  await dialog.getByRole("combobox", {name: "Provider"}).press("Enter");
  await page.getByRole("option", {name: "OpenAI", exact: true}).press("Enter");
  await dialog.getByRole("radio", {name: "Add new key"}).focus();
  await page.keyboard.down("ArrowRight");
  try { await expect(dialog.getByRole("radio", {name: "Use saved key"})).toBeChecked(); }
  finally { await page.keyboard.up("ArrowRight"); }
  await expect(dialog.getByRole("combobox", {name: "Saved key"})).toBeEnabled();
  await expect(dialog.getByLabel("API key", {exact: true})).toHaveCount(0);
}

async function match(actual: Locator, reference: Locator, properties: string[]) {
  const styles = (locator: Locator) => locator.evaluate((element, names) => {
    const css = getComputedStyle(element);
    return Object.fromEntries(names.map(name => {
      if (name === "width" || name === "height") return [name, Math.round(element.getBoundingClientRect()[name])];
      const value = css.getPropertyValue(name);
      return [name, name === "font-family" ? value.split(",")[0]?.replaceAll('"', "").replace(" Variable", "") : value];
    }));
  }, properties);
  await expect.poll(() => styles(actual), {message: actual.toString()}).toEqual(await styles(reference));
}

async function topOffset(from: Locator, to: Locator) {
  const a = (await from.boundingBox())!, b = (await to.boundingBox())!;
  return Math.round(b.y - a.y);
}

async function capture(page: Page, reference: Page, info: TestInfo, state: string, theme: string) {
  for (const [label, surface] of [["app", page], ["reference", reference]] as const) {
    expect(await surface.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
    const path = info.outputPath(`${label}-${state}-${theme}.png`);
    await surface.screenshot({path, animations: "disabled", scale: "css"});
  }
}
