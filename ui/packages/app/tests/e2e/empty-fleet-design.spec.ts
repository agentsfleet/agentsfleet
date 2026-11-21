import {expect, type Locator} from "@playwright/test";
import {test} from "./fixtures/chat-probe";
import starters from "@/public/fleet-starters.json" with {type: "json"};

test("empty fleet has the reference reading scale, editable Chief of Staff example and aligned rail mark", async ({page, chatProbe, isMobile}, info) => {
  if (!isMobile) await page.setViewportSize({width: 1440, height: 1000});
  await page.goto(chatProbe + "?emptySetup=1");
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
  const form = page.getByRole("form", {name:"Setup chat composer"});
  const caption = "No fleet selected. Start something new, or open your fleets from the rail.";
  await expect(heading).toBeVisible();
  await expect(page.getByRole("button", {name:"Create a plugin"})).toHaveCount(0);
  for (const theme of ["dark","light"]) {
    for (const surface of [page,design]) await surface.evaluate(t => {document.documentElement.dataset.theme=t;}, theme);
    await expect(referenceStarter.locator("svg")).toBeVisible();
    await page.emulateMedia({reducedMotion:"reduce"});
    await design.emulateMedia({reducedMotion:"reduce"});
    const pairs = [[heading,referenceHeading],[heading.locator("..").locator("p"),design.locator(".empty-heading p")],
      [form,design.locator(".empty-workspace .composer")],[input,design.locator(".empty-workspace textarea")],
      [starter,referenceStarter],[page.getByText(caption),design.getByText(caption)]] as const;
    const appTop=(await geometry(heading)).top, designTop=(await geometry(referenceHeading)).top;
    for (const [app, reference] of pairs) {
      const actual=await geometry(app), expected=await geometry(reference);
      expect(actual.top-appTop).toBeCloseTo(expected.top-designTop,0);
      expect(actual.height).toBe(expected.height);
      expect([actual.fontSize,actual.weight,actual.paddingLeft]).toEqual([expected.fontSize,expected.weight,expected.paddingLeft]);
      expect(Math.abs(actual.lineHeight-expected.lineHeight)).toBeLessThan(0.5);
    }
    await expect(form).toHaveCSS("background-color",theme==="dark" ? "rgb(32, 35, 40)" : "rgb(240, 242, 245)");
    const actualForm=await geometry(form), expectedForm=await geometry(design.locator(".empty-workspace .composer"));
    expect([actualForm.width,actualForm.radius]).toEqual([expectedForm.width,expectedForm.radius]);
    const logoBox=await geometry(logo), refMark=await geometry(referenceLogo);
    const selected=await geometry(page.getByRole("navigation",{name:"Primary"}).getByRole("link",{name:"New fleet",exact:true}));
    expect([logoBox.width,logoBox.height]).toEqual([selected.height,selected.height]);
    expect([logoBox.width,logoBox.height]).toEqual([refMark.width,refMark.height]);
    await page.screenshot({path:info.outputPath("empty-app-"+theme+".png"),animations:"disabled",scale:"css"});
    await design.screenshot({path:info.outputPath("empty-reference-"+theme+".png"),animations:"disabled",scale:"css"});
  }
  await page.getByRole("button",{name:"Mention a fleet in setup"}).click();
  await page.getByRole("option",{name:"@bob",exact:true}).click();
  await expect(input).toHaveValue("@bob ");
  await page.getByLabel("Choose a text document").setInputFiles({name:"handoff.md",mimeType:"text/plain",buffer:Buffer.from("Ask for the deploy status")});
  await expect(input).toHaveValue(/@bob .*handoff.md[\s\S]*deploy status/s);
  await input.fill("Ask NAME for a receipt");
  await input.evaluate(field => (field as HTMLTextAreaElement).setSelectionRange(4, 8));
  await page.getByRole("button",{name:"Mention a fleet in setup"}).click();
  await page.getByRole("option",{name:"@bob",exact:true}).click();
  await expect(input).toHaveValue("Ask @bob for a receipt");
  await input.fill("Ask @bo");
  await input.press("Escape");
  await page.getByRole("button",{name:"Mention a fleet in setup"}).click();
  await expect(input).toHaveValue("Ask @bo");
  await page.getByRole("option",{name:"@bob",exact:true}).click();
  await expect(input).toHaveValue("Ask @bob ");
  await input.fill("");
  await starter.click();
  const prompt = starters.find(example => example.title === "Set up a Chief of Staff")!.prompt;
  await expect(input).toHaveValue(prompt);
  await expect(heading).toBeVisible();
  await expect(page.getByRole("button",{name:"New fleet from message"})).toBeEnabled();
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
