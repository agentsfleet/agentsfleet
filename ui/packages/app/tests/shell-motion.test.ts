import { describe, expect, it } from "vitest";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";

// The dashboard's motion pass (content mount-rise, ambient glow-drift, hover /
// press lifts, nav nudge) is a scoped exception to the design system's "no
// performance" restraint — sanctioned ONLY because every effect is wholly
// disabled under prefers-reduced-motion. jsdom/happy-dom cannot resolve
// @media (prefers-reduced-motion) into a computed style, so — following the
// repo's CSS-as-source-of-truth motion tests (design-system Terminal /
// WakePulse) — this pins the gate structurally against the stylesheet and the
// Shell wiring: each animation exists, and each is disabled under reduced motion.

const APP_ROOT = resolve(__dirname, "..");

const GLOBALS = readFileSync(resolve(APP_ROOT, "app/globals.css"), "utf8");
const SHELL = readFileSync(resolve(APP_ROOT, "components/layout/Shell.tsx"), "utf8");

describe("dashboard motion is defined", () => {
  it("declares the content mount-rise keyframe and applies it to the content container's children", () => {
    expect(GLOBALS).toMatch(/@keyframes\s+rise-in\b/);
    // The rise targets the direct children of the .app-content-rise container.
    expect(GLOBALS).toMatch(/\.app-content-rise\s*>\s*\*\s*\{[^}]*animation:\s*rise-in/s);
  });

  it("declares the ambient glow-drift keyframe and applies it to the dashboard canvas", () => {
    expect(GLOBALS).toMatch(/@keyframes\s+glow-drift\b/);
    expect(GLOBALS).toMatch(/\.app-dashboard-canvas\s*\{[^}]*animation:\s*glow-drift/s);
    expect(GLOBALS).toMatch(/\.app-dashboard-canvas\s*\{[^}]*radial-gradient\(1150px 660px/s);
  });

  it("never pins opacity:0 as a resting state — rise-in degrades to visible", () => {
    // `both` fill must resolve to the visible `to` frame; the rule itself must
    // not set a bare opacity:0 that would survive if the animation never runs.
    expect(GLOBALS).toMatch(/animation:\s*rise-in[^;]*\bboth\b/);
    expect(GLOBALS).not.toMatch(/\.app-content-rise\s*>\s*\*\s*\{[^}]*opacity:\s*0\s*;/s);
  });

  it("wires the mount-rise container onto the Shell content wrapper", () => {
    // Same element that carries the shared content width — so it rises, not chrome.
    expect(SHELL).toMatch(/app-content-rise[^"]*max-w-content|max-w-content[^"]*app-content-rise/);
  });

  it("wires the dual-glow canvas onto the Shell main region", () => {
    expect(SHELL).toMatch(/<main className="app-dashboard-canvas/);
  });
});

describe("test_motion_respects_reduced_motion — every effect is gated", () => {
  it("neutralizes all keyframe animations + transitions under prefers-reduced-motion: reduce", () => {
    const reduceBlock = GLOBALS.match(
      /@media\s*\(prefers-reduced-motion:\s*reduce\)\s*\{([\s\S]*?)\}\s*\}/,
    );
    expect(reduceBlock, "globals.css must carry a prefers-reduced-motion: reduce block").not.toBeNull();
    const body = reduceBlock![1];
    // Applies to every element + pseudo so rise-in / glow-drift are covered.
    expect(body).toMatch(/\*\s*,\s*\n?\s*\*::before\s*,\s*\n?\s*\*::after/);
    expect(body).toMatch(/animation-duration:\s*0\.01ms\s*!important/);
    expect(body).toMatch(/animation-iteration-count:\s*1\s*!important/);
    expect(body).toMatch(/transition-duration:\s*0\.01ms\s*!important/);
  });

  it("scopes the hover/press lifts behind no-preference so they vanish under reduced-motion", () => {
    const noPref = GLOBALS.match(
      /@media\s*\(prefers-reduced-motion:\s*no-preference\)\s*\{([\s\S]*?\}\s*)\}/,
    );
    expect(noPref, "hover/press lifts must sit inside a no-preference query").not.toBeNull();
    const body = noPref![1];
    expect(body).toMatch(/\.app-glow-surface button:hover\s*\{[^}]*filter:\s*brightness/s);
    expect(body).toMatch(/\.app-glow-surface button:active\s*\{[^}]*transform:\s*translateY/s);
    expect(body).toMatch(/\.app-dashboard-canvas \[data-dashboard-panel\]/);
    expect(body).toMatch(/\.app-dashboard-canvas \[data-terminal-panel\]/);
  });

  it("gates the sidebar hover nudge with the motion-safe variant", () => {
    expect(SHELL).toContain("motion-safe:hover:translate-x-px");
  });
});
