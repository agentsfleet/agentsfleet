import { describe, it, expect } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import { AnimatedTerminal } from "./animated-terminal";

/*
 * SSR contract for AnimatedTerminal (§5.8.4 dim 5.8.15 + 5.8.16).
 * The component must render its chrome + initial prompt without
 * touching IntersectionObserver / AudioContext / matchMedia — all of
 * which are undefined during server render. renderToStaticMarkup
 * exercises that path here. Phase-machine and step-through behavior
 * live in the Playwright smoke spec.
 */

describe("AnimatedTerminal — SSR", () => {
  it("renders chrome + aria region without touching window APIs", () => {
    const html = renderToStaticMarkup(
      <AnimatedTerminal
        commands={[
          "zombiectl login",
          "zombiectl zombie install --template platform-ops",
        ]}
      />,
    );
    expect(html).toContain('role="region"');
    expect(html).toContain("Interactive terminal demonstration");
    // Prompt username default renders even pre-intersection.
    expect(html).toContain("you@usezombie");
    // macOS chrome traffic lights
    expect(html).toContain("rounded-full bg-destructive/70");
    expect(html).toContain("rounded-full bg-warning/70");
    expect(html).toContain("rounded-full bg-success/70");
  });

  it("live-announces output to assistive tech", () => {
    const html = renderToStaticMarkup(<AnimatedTerminal commands={["ls"]} />);
    expect(html).toContain('aria-live="polite"');
  });

  it("does not emit any /sounds/ request markers (audio was scoped out)", () => {
    const html = renderToStaticMarkup(
      <AnimatedTerminal commands={["zombiectl login"]} />,
    );
    expect(html).not.toContain("/sounds/");
    expect(html).not.toContain("AudioContext");
  });

  it("falls through to the default prompt when override is an empty string", () => {
    // Regression test for the `if (override)` (vs `!== undefined`) change:
    // a caller passing prompts={2: ""} must not render a blank prompt
    // line. The `you@usezombie $` default has to win for any falsy
    // override value.
    const reducedMotionMatch = window.matchMedia;
    window.matchMedia = ((query: string) => ({
      matches: query.includes("reduce"),
      media: query,
      onchange: null,
      addEventListener: () => {},
      removeEventListener: () => {},
      addListener: () => {},
      removeListener: () => {},
      dispatchEvent: () => false,
    })) as unknown as typeof window.matchMedia;

    try {
      const html = renderToStaticMarkup(
        <AnimatedTerminal
          commands={["zombiectl login"]}
          prompts={{ 0: "" }}
        />,
      );
      // Default prompt survives despite the empty override.
      expect(html).toContain("you@usezombie");
      // Negative: an empty <span class="text-warning"> ... </span> would
      // appear here if the fallthrough broke. Match the exact warning-only
      // empty-prompt shape so the assertion fails loudly on regression.
      expect(html).not.toMatch(/<span[^>]*class="text-warning"[^>]*>\s*<\/span>/);
    } finally {
      window.matchMedia = reducedMotionMatch;
    }
  });

  it("renders prompt overrides verbatim under reduced motion", () => {
    // SSR + reduced-motion is the deterministic path that exercises
    // buildInstantLines — guarantees the prompts prop reaches the rendered
    // output even when the phase machine never runs.
    const reducedMotionMatch = window.matchMedia;
    window.matchMedia = ((query: string) => ({
      matches: query.includes("reduce"),
      media: query,
      onchange: null,
      addEventListener: () => {},
      removeEventListener: () => {},
      addListener: () => {},
      removeListener: () => {},
      dispatchEvent: () => false,
    })) as unknown as typeof window.matchMedia;

    try {
      const html = renderToStaticMarkup(
        <AnimatedTerminal
          commands={["zombiectl login", "/usezombie-install-platform-ops"]}
          prompts={{ 1: "claude-code ›" }}
        />,
      );
      // Override appears in place of the default `you@usezombie $` prompt
      // for index 1, while index 0 keeps the default.
      expect(html).toContain("claude-code ›");
      expect(html).toContain("you@usezombie");
    } finally {
      window.matchMedia = reducedMotionMatch;
    }
  });
});
