import {expect, type Locator} from "@playwright/test";
import {test} from "./fixtures/chat-probe";
import starters from "@/public/fleet-starters.json" with {type: "json"};
import {FLEET_DETAIL} from "@/tests/helpers/fleet-fixtures";

test("empty fleet has the reference reading scale, editable Chief of Staff example and aligned rail mark", async ({page, chatProbe, isMobile}, info) => {
  if (!isMobile) await page.setViewportSize({width: 1440, height: 1000});
  // A workspace fleet the composer can mention while the first message is written.
  await page.route("**/live/v1/workspaces/*/fleets?*", route => route.fulfill({json: {items: [{...FLEET_DETAIL, name: "bob"}], total: null, next_cursor: null}}));
  await page.goto(chatProbe + "?newFleet=1");
  const design = await page.context().newPage();
  const size=page.viewportSize();
  if (size) await design.setViewportSize(size);
  await design.goto(new URL("../../../../../docs/ux.html#empty", import.meta.url).href);
  const heading = page.getByRole("heading", {name:"What should your fleet do?"});
  const referenceHeading = design.locator(".empty-heading h2");
  const input = page.getByRole("textbox", {name:"What would you like your fleet to do?"});
  const starter = page.getByRole("button", {name:"Set up a Chief of Staff"});
  const referenceStarter = design.getByRole("button", {name:"Set up a Chief of Staff"});
  const logo = page.getByRole("link", {name:"agentsfleet home"}).locator("svg");
  const referenceLogo = design.locator(".rail .brand img");
  const newFleet = page.getByRole("navigation", {name: "Primary"}).getByRole("link", {name: "New fleet", exact: true});
  const referenceNewFleet = design.locator(".rail .new-fleet");
  const form = page.getByRole("form", {name:"New fleet composer"});
  const caption = "No fleet selected. Start something new, or open your fleets from the rail.";
  await expect(heading).toBeVisible();
  await expect(page.getByRole("button", {name:"Create a plugin"})).toHaveCount(0);
  for (const theme of ["dark","light"]) {
    for (const surface of [page,design]) await surface.evaluate(t => {document.documentElement.dataset.theme=t;}, theme);
    await expect(referenceStarter.locator("svg")).toBeVisible();
    await page.emulateMedia({reducedMotion:"reduce"});
    await design.emulateMedia({reducedMotion:"reduce"});
    // Above the composer the page keeps the reference's positions; the composer is the fleet chat's own, so
    // what sits under it keeps the reference's type and size but moves with the composer's height.
    const placed = [[heading,referenceHeading],[heading.locator("..").locator("p"),design.locator(".empty-heading p")]] as const;
    const styled = [[input,design.locator(".empty-workspace textarea")],[starter,referenceStarter],[page.getByText(caption),design.getByText(caption)]] as const;
    const appTop=(await geometry(heading)).top, designTop=(await geometry(referenceHeading)).top;
    for (const [app, reference] of [...placed, ...styled]) {
      const actual=await geometry(app), expected=await geometry(reference);
      if (placed.some(([placedApp]) => placedApp === app)) expect(actual.top-appTop).toBeCloseTo(expected.top-designTop,0);
      expect(actual.height).toBe(expected.height);
      expect([actual.fontSize,actual.weight,actual.paddingLeft]).toEqual([expected.fontSize,expected.weight,expected.paddingLeft]);
      expect(Math.abs(actual.lineHeight-expected.lineHeight)).toBeLessThan(0.5);
    }
    await expect(form).toHaveCSS("background-color",theme==="dark" ? "rgb(32, 35, 40)" : "rgb(240, 242, 245)");
    const actualMention = await geometry(form.getByRole("button",{name:"Mention a fleet"}).locator("svg"));
    const expectedMention = await geometry(design.locator('.composer [data-action="mention"] i'));
    expect([actualMention.width,actualMention.height]).toEqual([expectedMention.width,expectedMention.height]);
    const actualForm=await geometry(form), expectedForm=await geometry(design.locator(".empty-workspace .composer"));
    expect([actualForm.width,actualForm.radius]).toEqual([expectedForm.width,expectedForm.radius]);
    const logoBox=await geometry(logo), refMark=await geometry(referenceLogo);
    const selected=await geometry(newFleet);
    expect([logoBox.width,logoBox.height]).toEqual([selected.height,selected.height]);
    expect([logoBox.width,logoBox.height]).toEqual([refMark.width,refMark.height]);
    await expect(newFleet).toHaveCSS("background-color", await referenceNewFleet.evaluate(node => getComputedStyle(node).backgroundColor));
    await expect(newFleet).toHaveCSS("color", await referenceNewFleet.evaluate(node => getComputedStyle(node).color));
    await page.screenshot({path:info.outputPath("empty-app-"+theme+".png"),animations:"disabled",scale:"css"});
    await design.screenshot({path:info.outputPath("empty-reference-"+theme+".png"),animations:"disabled",scale:"css"});
  }
  // A first message mentions another fleet where the caret is; it never becomes the message's recipient.
  const mention = form.getByRole("button",{name:"Mention a fleet"});
  await mention.click();
  await page.getByRole("option",{name:"@bob",exact:true}).click();
  await expect(input).toHaveValue("@bob ");
  await input.fill("Ask NAME for a receipt");
  await input.evaluate(field => (field as HTMLTextAreaElement).setSelectionRange(4, 8));
  await mention.click();
  await page.getByRole("option",{name:"@bob",exact:true}).click();
  await expect(input).toHaveValue("Ask @bob for a receipt");
  await input.fill("Ask @bo");
  await input.press("Escape");
  await mention.click();
  await expect(input).toHaveValue("Ask @bo");
  await page.getByRole("option",{name:"@bob",exact:true}).click();
  await expect(input).toHaveValue("Ask @bob ");
  await input.fill("");
  await starter.click();
  const prompt = starters.find(example => example.title === "Set up a Chief of Staff")!.prompt;
  await expect(input).toHaveValue(prompt);
  await expect(heading).toBeVisible();
  await expect(form.getByRole("button",{name:"Send", exact: true})).toBeEnabled();
  await referenceStarter.click();
  await expect(design.locator(".empty-workspace textarea")).toHaveValue(prompt);
  await design.close();
});

function geometry(element: Locator) {
  return element.evaluate(node=>{
    const box=node.getBoundingClientRect(), css=getComputedStyle(node);
    return {width:Math.round(box.width),height:Math.round(box.height),top:Math.round(box.y),fontSize:css.fontSize,
      lineHeight:parseFloat(css.lineHeight),weight:css.fontWeight,paddingLeft:css.paddingLeft,radius:css.borderTopLeftRadius};
  });
}
