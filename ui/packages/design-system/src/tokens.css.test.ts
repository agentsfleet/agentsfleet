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

// dark-mode resting-state border/surface contrast bump. Same
// text-contract rationale as above: jsdom applies no real CSS, so this pins
// the source value directly against a silent revert to the pre-bump flat
// tokens.
describe("tokens.css — dark-mode contrast bump", () => {
  const css = readFileSync(TOKENS_CSS_PATH, "utf8");
  const rootStart = css.indexOf(":root {");
  // The light-mode selector also appears earlier inside a prose comment
  // (line ~6), so the search for the *block* must start after :root's own
  // opening brace to avoid slicing an inverted (empty) range.
  const lightStart = css.indexOf('[data-theme="light"] {', rootStart);
  const rootBlock = css.slice(rootStart, lightStart);
  const lightBlock = css.slice(lightStart);

  it("bumps dark-mode --border to #2b333a", () => {
    expect(rootBlock).toContain("--border: #2b333a;");
    expect(rootBlock).not.toContain("--border: #23292e;");
  });

  it("bumps dark-mode --surface-1 to #141a1f", () => {
    expect(rootBlock).toContain("--surface-1: #141a1f;");
    expect(rootBlock).not.toContain("--surface-1: #11161a;");
  });

  it("leaves light-mode --border/--surface-1 untouched", () => {
    expect(lightBlock).toContain("--surface-1: #f1eee6;");
    expect(lightBlock).toContain("--border: #d4cdb9;");
  });
});
