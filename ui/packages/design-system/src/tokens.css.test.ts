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
