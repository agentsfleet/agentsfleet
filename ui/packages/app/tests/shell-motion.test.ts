import { describe, expect, it } from "vitest";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";

// The dashboard no longer animates page mounts or the ambient canvas: loading
// feedback lives in RouteLoading's spinner, and navigation should not wobble.
// jsdom/happy-dom cannot resolve @media (prefers-reduced-motion) into a
// computed style, so this pins the stylesheet and layout wiring structurally.

const APP_ROOT = resolve(__dirname, "..");

const GLOBALS = readFileSync(resolve(APP_ROOT, "app/globals.css"), "utf8");
const SHELL = readFileSync(resolve(APP_ROOT, "components/layout/Shell.tsx"), "utf8");
const SIDEBAR_NAVIGATION = readFileSync(
  resolve(APP_ROOT, "components/layout/SidebarNavigation.tsx"),
  "utf8",
);

describe("dashboard route motion is absent", () => {
  it("keeps page mounts and the ambient canvas static", () => {
    expect(GLOBALS).not.toMatch(/@keyframes\s+rise-in\b/);
    expect(GLOBALS).not.toMatch(/\.app-content-rise\s*>\s*\*\s*\{/);
    expect(GLOBALS).not.toMatch(/@keyframes\s+glow-drift\b/);
    expect(GLOBALS).not.toMatch(/\.app-dashboard-canvas\s*\{[^}]*animation:\s*glow-drift/s);
    // A single restrained brand glow (top-right); the former multi-stop teal/blue
    // field was reduced to one calm radial per the Operational-Restraint pass.
    expect(GLOBALS).toMatch(/\.app-dashboard-canvas\s*\{[^}]*radial-gradient\(1000px 620px/s);
  });

  it("never wires a page-mount animation onto the Shell content wrapper", () => {
    expect(SHELL).not.toContain("app-content-rise");
    expect(GLOBALS).not.toMatch(/\.app-content-rise\s*>\s*\*\s*\{[^}]*opacity:\s*0\s*;/s);
  });

  it("wires the ambient-glow canvas onto the Shell main region", () => {
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
    // Applies to every element + pseudo so meter-fill and future additions are
    // covered.
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
    expect(SIDEBAR_NAVIGATION).toContain("motion-safe:hover:translate-x-px");
  });
});
