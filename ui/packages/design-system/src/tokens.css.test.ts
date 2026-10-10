import { readFileSync } from "node:fs";
import path from "node:path";
import { describe, expect, it } from "vitest";

const TOKENS_CSS_PATH = path.join(__dirname, "tokens.css");

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
const FOREGROUNDS = ["text", "text-muted", "text-subtle", "pulse", "success", "warn", "error", "info"];

function luminance(hex: string): number {
  const channels = hex.match(/[a-f0-9]{2}/gi);
  if (!channels || channels.length !== LUMINANCE_WEIGHTS.length) throw new Error("Expected six-digit RGB");
  return channels.reduce((sum, channel, index) => {
    const value = parseInt(channel, 16) / MAX_RGB_CHANNEL;
    const linear = value <= SRGB_THRESHOLD ? value / SRGB_DIVISOR : ((value + SRGB_OFFSET) / SRGB_SCALE) ** SRGB_POWER;
    return sum + linear * (LUMINANCE_WEIGHTS[index] ?? 0);
  }, 0);
}

describe("theme contrast pairs", () => {
  const css = readFileSync(TOKENS_CSS_PATH, "utf8");
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
});
