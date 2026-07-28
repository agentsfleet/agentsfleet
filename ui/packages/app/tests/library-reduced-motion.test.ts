import { describe, expect, it } from "vitest";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";

// jsdom and happy-dom cannot resolve `@media (prefers-reduced-motion)` into a
// computed style, so — like tests/shell-motion.test.ts — this pins the wiring
// structurally rather than pretending to observe an animation.
//
// The guarantee under test has two halves, and both matter:
//   1. the loading regions introduce NO motion of their own, so they inherit
//      the global reduce block instead of escaping it, and
//   2. loading stays distinguishable from loaded WITHOUT relying on motion —
//      a user who has asked for stillness must still be able to tell that
//      something is arriving, which a pure shimmer cue cannot provide.

const APP_ROOT = resolve(__dirname, "..");
const read = (rel: string) => readFileSync(resolve(APP_ROOT, rel), "utf8");

const GLOBALS = read("app/globals.css");
const FLEET_PAGE = read("app/(dashboard)/w/[workspaceId]/fleets/new/page.tsx");
const MODELS_PAGE = read("app/(dashboard)/w/[workspaceId]/settings/models/page.tsx");
const LOADING_REGIONS = [FLEET_PAGE, MODELS_PAGE];

describe("test_library_reduced_motion_state — library loading honours reduced motion", () => {
  it("neutralizes animation for every element, which is what the skeletons rely on", () => {
    // `Skeleton` ships a bare `animate-pulse`, not `motion-safe:animate-pulse`.
    // That is only acceptable because this block covers the universal selector
    // with `!important`. If the block were ever narrowed to named classes, the
    // library skeletons would start pulsing at users who asked them not to.
    const reduceBlock = GLOBALS.match(
      /@media\s*\(prefers-reduced-motion:\s*reduce\)\s*\{([\s\S]*?)\}\s*\}/,
    );
    expect(reduceBlock, "globals.css must carry a prefers-reduced-motion: reduce block").not.toBeNull();

    const body = reduceBlock![1]!;
    expect(body, "the reduce block must cover the universal selector").toMatch(/\*\s*,/);
    expect(body).toMatch(/animation-duration:\s*0\.01ms\s*!important/);
    expect(body).toMatch(/animation-iteration-count:\s*1\s*!important/);
    expect(body).toMatch(/transition-duration:\s*0\.01ms\s*!important/);
  });

  it("introduces no bespoke animation in either library loading region", () => {
    // A hand-rolled shimmer or a local @keyframes would sit outside the
    // design-system primitive and could outlive a future narrowing of the
    // global block. The regions must borrow motion, never mint it.
    for (const source of LOADING_REGIONS) {
      expect(source).not.toMatch(/@keyframes/);
      expect(source).not.toMatch(/animate-(?!pulse\b)[a-z-]+/);
      expect(source).not.toMatch(/transition-[a-z]+\s/);
    }
  });

  it("uses the design-system Skeleton rather than a local placeholder", () => {
    for (const source of LOADING_REGIONS) {
      expect(source).toMatch(/from "@agentsfleet\/design-system"/);
      expect(source).toMatch(/<Skeleton\b/);
    }
  });

  it("marks loading with aria-busy so it is distinguishable without motion", () => {
    // The non-negotiable half. Under reduced motion the pulse is frozen, so a
    // sighted user sees a static grey block and a screen-reader user hears
    // nothing at all unless the region announces itself. `aria-busy` plus a
    // label is what keeps "loading" different from "loaded and empty".
    for (const source of LOADING_REGIONS) {
      expect(source).toMatch(/aria-busy="true"/);
      expect(source).toMatch(/aria-label="Loading [^"]+"/);
    }
  });
});
