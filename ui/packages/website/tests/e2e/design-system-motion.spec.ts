import { expect, test, type Page } from "@playwright/test";

/*
 * The design system's attribute-driven motion, proven on the compiled CSS in
 * Chromium. jsdom never applies a stylesheet, so a selector that loosened to a
 * bare presence match (`[data-live]` firing for `data-live="false"`) or a lost
 * reduced-motion override is only visible here, in a computed style.
 */

const GALLERY_ROUTE = "/_design-system";
const LITERAL_TRUE = "true";
const LITERAL_FALSE = "false";
const PRESENCE_ONLY = "";
const NO_ANIMATION = "none";
const NO_SHADOW = "none";
const FULL_OPACITY = "1";

const LIVE = "data-live";
const BECKON = "data-beckon";
const BECKON_TEXT = "data-beckon-text";
const TOOL_SHIMMER = "data-tool-shimmer";
const SETTLED = "data-settled";
const SKIP_LAYOUT = "data-skip-layout";
const BRAILLE_SPINNER = "data-braille-spinner";
const BRAILLE_FRAMES = "data-braille-frames";
const BRAILLE_REST = "data-braille-rest";

const WAKE_PULSE = "wake-pulse";
const CTA_BECKON = "cta-beckon";
const TEXT_BECKON = "text-beckon";
const TOOL_SHIMMER_KEYFRAMES = "tool-shimmer";
const BRAILLE_SPIN = "braille-spin";
const BRAILLE_FRAME_COUNT = 10;
const BRAILLE_CYCLE = "0.8s";

const FOCUS_RING_REACH = "--focus-ring-reach";
const MOTION_DURATION_STREAM = "--motion-duration-stream";
const TRANSITION_DURATION_STREAM = "--transition-duration-stream";
const PULSE = "--pulse";
const FONT_DISPLAY_TOKEN = "--ff-display";
const PROBE_FONT = "Georgia";
const PROBE_DURATION = "200ms";
const RING_REACH_DELTA_PX = 10;
const VISIBLE_CONTENT = "visible";
const AUTO_CONTENT = "auto";

type Style = Record<string, string>;

/** Mounts a span carrying `attrs` / `className` and returns its computed `props`. */
async function styleOf(page: Page, attrs: Record<string, string>, props: string[], className = ""): Promise<Style> {
  return page.evaluate(
    ({ attrs, props, className }) => {
      const el = document.createElement("span");
      el.className = className;
      for (const [name, value] of Object.entries(attrs)) el.setAttribute(name, value);
      document.body.append(el);
      const style = getComputedStyle(el);
      const out = Object.fromEntries(props.map((p) => [p, style.getPropertyValue(p).trim()]));
      el.remove();
      return out;
    },
    { attrs, props, className },
  );
}

async function animationOf(page: Page, attr: string, value: string): Promise<Style> {
  return styleOf(page, { [attr]: value }, ["animation-name", "animation-duration"]);
}

async function rootProperty(page: Page, name: string): Promise<string> {
  return page.evaluate((n) => getComputedStyle(document.documentElement).getPropertyValue(n).trim(), name);
}

async function setRootProperty(page: Page, name: string, value: string): Promise<void> {
  await page.evaluate(([n, v]) => document.documentElement.style.setProperty(n, v), [name, value]);
}

/** The colour a probe painted with `var(<token>)` resolves to, for comparing against a computed colour. */
async function tokenColor(page: Page, token: string): Promise<string> {
  return page.evaluate((t) => {
    const el = document.createElement("span");
    el.style.color = `var(${t})`;
    document.body.append(el);
    const color = getComputedStyle(el).color;
    el.remove();
    return color;
  }, token);
}

test.beforeEach(async ({ page }) => {
  await page.goto(GALLERY_ROUTE);
});

test.describe("attribute-driven motion fires only for the literal true value", () => {
  for (const [attr, keyframes] of [
    [LIVE, WAKE_PULSE],
    [BECKON, CTA_BECKON],
    [BECKON_TEXT, TEXT_BECKON],
    [TOOL_SHIMMER, TOOL_SHIMMER_KEYFRAMES],
  ] as const) {
    test(`${attr}="true" runs ${keyframes}; "false" and a bare ${attr} stay still`, async ({ page }) => {
      expect((await animationOf(page, attr, LITERAL_TRUE))["animation-name"]).toBe(keyframes);
      expect((await animationOf(page, attr, LITERAL_FALSE))["animation-name"]).toBe(NO_ANIMATION);
      expect((await animationOf(page, attr, PRESENCE_ONLY))["animation-name"]).toBe(NO_ANIMATION);
    });
  }

  test("the beckon is its own keyframe, at the live pulse's cadence, as are the beckon text and tool shimmer", async ({ page }) => {
    const live = await animationOf(page, LIVE, LITERAL_TRUE);
    const beckon = await animationOf(page, BECKON, LITERAL_TRUE);
    expect(beckon["animation-name"]).not.toBe(live["animation-name"]);
    for (const attr of [BECKON, BECKON_TEXT, TOOL_SHIMMER]) {
      expect((await animationOf(page, attr, LITERAL_TRUE))["animation-duration"], attr).toBe(live["animation-duration"]);
    }
  });

  test("only explicit history layout skipping opts in; completed live rows keep their layout", async ({ page }) => {
    expect((await styleOf(page, { [SKIP_LAYOUT]: LITERAL_TRUE }, ["content-visibility"]))["content-visibility"]).toBe(AUTO_CONTENT);
    expect((await styleOf(page, { [SKIP_LAYOUT]: LITERAL_FALSE }, ["content-visibility"]))["content-visibility"]).toBe(VISIBLE_CONTENT);
    expect((await styleOf(page, { [SKIP_LAYOUT]: PRESENCE_ONLY }, ["content-visibility"]))["content-visibility"]).toBe(VISIBLE_CONTENT);
    expect((await styleOf(page, { [SETTLED]: LITERAL_TRUE }, ["content-visibility"]))["content-visibility"]).toBe(VISIBLE_CONTENT);
  });
});

test.describe("tokens derive from one declaration and the spinner steps in CSS", () => {
  test("a history row's ring room grows with the focus ring's reach and is cancelled by its margin", async ({ page }) => {
    const roomAt = async (reach: string) => {
      await setRootProperty(page, FOCUS_RING_REACH, reach);
      const style = await styleOf(page, { [SKIP_LAYOUT]: LITERAL_TRUE }, ["padding-left", "margin-left"]);
      expect(parseFloat(style["margin-left"] ?? "")).toBe(-parseFloat(style["padding-left"] ?? ""));
      return parseFloat(style["padding-left"] ?? "");
    };
    const without = await roomAt("0px");
    const withReach = await roomAt(`${RING_REACH_DELTA_PX}px`);
    expect(withReach - without).toBe(RING_REACH_DELTA_PX);
  });

  test("the stream transition utility follows the stream motion token", async ({ page }) => {
    expect(await rootProperty(page, TRANSITION_DURATION_STREAM)).toBe(await rootProperty(page, MOTION_DURATION_STREAM));
    await setRootProperty(page, MOTION_DURATION_STREAM, PROBE_DURATION);
    expect(await rootProperty(page, TRANSITION_DURATION_STREAM)).toBe(PROBE_DURATION);
  });

  test("display, interface, and technical font roles resolve independently", async ({ page }) => {
    const family = async (className: string) => (await styleOf(page, {}, ["font-family"], className))["font-family"] ?? "";
    const roles = ["font-display", "font-sans", "font-mono"];
    const before = await Promise.all(roles.map(family));
    expect(new Set(before).size).toBe(roles.length);
    await setRootProperty(page, FONT_DISPLAY_TOKEN, PROBE_FONT);
    const after = await Promise.all(roles.map(family));
    expect(after[0]).toBe(PROBE_FONT);
    expect(after.slice(1)).toEqual(before.slice(1));
  });

  test("the braille spinner steps its ten frames in one cycle inside a clipped window", async ({ page }) => {
    const frames = await styleOf(page, { [BRAILLE_FRAMES]: PRESENCE_ONLY }, ["animation-name", "animation-duration", "animation-timing-function"]);
    expect(frames["animation-name"]).toBe(BRAILLE_SPIN);
    expect(frames["animation-duration"]).toBe(BRAILLE_CYCLE);
    expect(frames["animation-timing-function"]).toMatch(new RegExp(`^steps\\(${BRAILLE_FRAME_COUNT}\\b`));
    expect((await styleOf(page, { [BRAILLE_SPINNER]: PRESENCE_ONLY }, ["overflow-y"]))["overflow-y"]).toBe("hidden");
    expect((await styleOf(page, { [BRAILLE_REST]: PRESENCE_ONLY }, ["display"]))["display"]).toBe("none");
  });
});

test.describe("reduced motion stops every loop and keeps a static mark", () => {
  test.beforeEach(async ({ page }) => {
    await page.emulateMedia({ reducedMotion: "reduce" });
  });

  for (const attr of [LIVE, BECKON]) {
    test(`${attr}="true" stops, keeps a static ring, and is not dimmed`, async ({ page }) => {
      const style = await styleOf(page, { [attr]: LITERAL_TRUE }, ["animation-name", "box-shadow", "opacity"]);
      expect(style["animation-name"]).toBe(NO_ANIMATION);
      expect(style["box-shadow"]).not.toBe(NO_SHADOW);
      expect(style.opacity).toBe(FULL_OPACITY);
    });
  }

  for (const attr of [BECKON_TEXT, TOOL_SHIMMER]) {
    test(`${attr}="true" stops and holds the live colour at full opacity`, async ({ page }) => {
      const style = await styleOf(page, { [attr]: LITERAL_TRUE }, ["animation-name", "color", "opacity"]);
      expect(style["animation-name"]).toBe(NO_ANIMATION);
      expect(style.color).toBe(await tokenColor(page, PULSE));
      expect(style.opacity).toBe(FULL_OPACITY);
    });
  }

  test("the braille spinner swaps its moving column for the rest frame", async ({ page }) => {
    const frames = await styleOf(page, { [BRAILLE_FRAMES]: PRESENCE_ONLY }, ["display", "animation-name"]);
    expect(frames).toEqual({ display: "none", "animation-name": NO_ANIMATION });
    expect((await styleOf(page, { [BRAILLE_REST]: PRESENCE_ONLY }, ["display"]))["display"]).toBe("inline");
  });
});
