import { readFileSync } from "node:fs";
import path from "node:path";
import { describe, expect, it } from "vitest";

// A permanent-glow bug was fixed: tokens.css's wake-pulse selector
// matched `[data-live]` (any value, including the literal string "false"),
// so a consumer rendering `data-live={live}` unconditionally glowed even
// when not live. jsdom (this package's test environment) never applies
// real CSS, so no rendered-animation assertion can observe the bug or a
// regression back to it — WakePulse.test.tsx and the app's
// active-model-row.test.tsx only prove which *consumers* set the attribute,
// never that the selector itself still requires the "true" value. This is
// a text-contract pin on the CSS source directly: it fails the moment
// someone reverts the selector to the bare, presence-only form.
const TOKENS_CSS_PATH = path.join(__dirname, "tokens.css");
const THEME_CSS_PATH = path.join(__dirname, "theme.css");

describe("tokens.css — wake-pulse [data-live] selector contract", () => {
  it('only animates the literal [data-live="true"] value', () => {
    const css = readFileSync(TOKENS_CSS_PATH, "utf8");
    expect(css).toContain('[data-live="true"] {');
  });

  it("never re-introduces a bare [data-live] presence selector driving the animation", () => {
    const css = readFileSync(TOKENS_CSS_PATH, "utf8");
    // Matches `[data-live] {` or `[data-live]{` but not `[data-live="true"] {`
    // (the `=` after data-live in the real selector keeps this from
    // false-positiving on the correct rule).
    expect(css).not.toMatch(/\[data-live\]\s*\{/);
  });
});

describe("operational stream duration token", () => {
  it("pins the 80ms stream entry duration and exposes its named utility", () => {
    const tokens = readFileSync(TOKENS_CSS_PATH, "utf8");
    const theme = readFileSync(THEME_CSS_PATH, "utf8");

    expect(tokens).toContain("--motion-duration-stream: 80ms;");
    expect(theme).toContain(
      "--transition-duration-stream: var(--motion-duration-stream);",
    );
  });
});

const MIN_TEXT_CONTRAST = 4.5;
const MAX_RGB_CHANNEL = 255;
const SRGB_THRESHOLD = 0.04045;
const SRGB_DIVISOR = 12.92;
const SRGB_OFFSET = 0.055;
const SRGB_SCALE = 1.055;
const SRGB_POWER = 2.4;
const CONTRAST_OFFSET = 0.05;
const LUMINANCE_WEIGHTS = [0.2126, 0.7152, 0.0722];
const SURFACES = ["bg", "surface-deep", "surface-1", "surface-2", "surface-3"];
const FOREGROUNDS = ["text", "text-muted", "text-subtle"];

function luminance(hex: string): number {
  const channels = hex.match(/[a-f0-9]{2}/gi);
  if (!channels || channels.length !== LUMINANCE_WEIGHTS.length) throw new Error("Expected six-digit RGB");
  return channels.reduce((sum, channel, index) => {
    const value = parseInt(channel, 16) / MAX_RGB_CHANNEL;
    const linear = value <= SRGB_THRESHOLD ? value / SRGB_DIVISOR : ((value + SRGB_OFFSET) / SRGB_SCALE) ** SRGB_POWER;
    return sum + linear * (LUMINANCE_WEIGHTS[index] ?? 0);
  }, 0);
}

describe("theme contrast pairs and font roles", () => {
  const css = readFileSync(TOKENS_CSS_PATH, "utf8");
  const theme = readFileSync(THEME_CSS_PATH, "utf8");
  const blocks = [
    { name: "dark", source: css.split(":root {")[1]?.split('[data-theme="light"]')[0] ?? "" },
    { name: "light", source: css.split('[data-theme="light"] {')[1]?.split("}")[0] ?? "" },
  ];

  for (const block of blocks) {
    const colors = Object.fromEntries([...block.source.matchAll(/--([\w-]+):\s*(#[a-f0-9]{6});/gi)].map((match) => [match[1], match[2]]));
    for (const foreground of FOREGROUNDS) {
      for (const surface of SURFACES) {
        it(`${block.name}: ${foreground} remains readable on ${surface}`, () => {
          const fg = colors[foreground];
          const bg = colors[surface];
          expect(fg, foreground).toBeDefined();
          expect(bg, surface).toBeDefined();
          const a = luminance(fg ?? "");
          const b = luminance(bg ?? "");
          expect((Math.max(a, b) + CONTRAST_OFFSET) / (Math.min(a, b) + CONTRAST_OFFSET)).toBeGreaterThanOrEqual(MIN_TEXT_CONTRAST);
        });
      }
    }
  }

  it("keeps display, interface, and technical font roles independent", () => {
    expect(css).toContain('--ff-display: "Bricolage Grotesque Variable"');
    expect(css).toContain('--ff-sans: "Instrument Sans Variable"');
    expect(css).toContain('--ff-mono: "Commit Mono"');
    for (const role of ["display", "sans", "mono"]) expect(theme).toContain(`--font-${role}: var(--ff-${role});`);
    expect(theme).not.toMatch(/(--[\w-]+):\s*var\(\1\)/);
  });

  it("does not dim live content for reduced motion", () => {
    expect(css).toMatch(/prefers-reduced-motion: reduce[\s\S]*?opacity: 1/);
    expect(css).not.toContain("opacity: 0.6");
  });
});
