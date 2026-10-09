import { describe, it, expect } from "vitest";
import {
  FORBIDDEN_MARKETING_CLAIMS,
  HERO_HEADLINE,
  HERO_LEDE_PARTS,
  PILLAR_TOKENS,
} from "./lib/marketing-copy";

/*
 * Guards the approved positioning copy: every PILLAR_TOKENS entry appears in
 * the hero (Hero.tsx source, HERO_HEADLINE and HERO_LEDE_PARTS), the canonical
 * npm install path exists somewhere in src/, and no FORBIDDEN_MARKETING_CLAIMS
 * or retired brand nouns survive in source copy.
 *
 * Test names follow RULE TST-NAM (no milestone IDs in test names).
 * Uses Vite import.meta.glob to stay browser-friendly in jsdom.
 */

const heroSource = import.meta.glob<string>("/src/components/Hero.tsx", {
  eager: true,
  query: "?raw",
  import: "default",
});

const allMarketingSources = import.meta.glob<string>(
  [
    "/src/**/*.{ts,tsx,js,jsx}",
    "!/src/**/*.test.{ts,tsx}",
    "!/src/**/*.spec.{ts,tsx}",
    "!/src/marketing-spec.test.ts",
  ],
  { eager: true, query: "?raw", import: "default" },
);

describe("marketing hero — compounding operational knowledge pillars present", () => {
  it("hero copy contains every current pillar token", () => {
    const heroFiles = Object.values(heroSource);
    expect(heroFiles, "Hero.tsx not found by import.meta.glob").toHaveLength(1);
    const body = [heroFiles[0], HERO_HEADLINE, ...Object.values(HERO_LEDE_PARTS)].join(" ");
    for (const token of PILLAR_TOKENS) {
      expect(body, `hero copy missing pillar token: ${token}`).toContain(token);
    }
  });
});

// The literal pin of the headline and the wake-on-event lede phrase: the
// other suites import those constants, so a revert of either would pass them.
describe("marketing hero — approved wording pinned once", () => {
  it("pins the approved headline and the wake-on-event lede phrase", () => {
    expect(HERO_HEADLINE).toBe("AI agents that wake when production breaks.");
    // pin test: literal is the contract
    expect(HERO_LEDE_PARTS.trigger).toBe("wakes on a production event");
  });
});

describe("marketing install command — npm path present", () => {
  it("at least one hit on `npm install -g @agentsfleet/cli` across src/", () => {
    const hits: string[] = [];
    for (const [path, body] of Object.entries(allMarketingSources)) {
      body.split("\n").forEach((line, i) => {
        if (line.includes("npm install -g @agentsfleet/cli")) {
          hits.push(`${path}:${i + 1}`);
        }
      });
    }
    expect(
      hits.length,
      `Expected ≥1 npm install command, found 0. Surfaces should carry the canonical install path.`,
    ).toBeGreaterThanOrEqual(1);
  });
});

describe("marketing overclaim guard", () => {
  it("contains zero unvalidated quantitative or autonomous-merge claims", () => {
    const hits: string[] = [];

    for (const [path, body] of Object.entries(allMarketingSources)) {
      let insideForbiddenClaimList = false;
      body.split("\n").forEach((line, i) => {
        if (line.includes("FORBIDDEN_MARKETING_CLAIMS")) {
          insideForbiddenClaimList = true;
          return;
        }
        if (insideForbiddenClaimList) {
          if (line.includes("] as const")) {
            insideForbiddenClaimList = false;
          }
          return;
        }
        for (const claim of FORBIDDEN_MARKETING_CLAIMS) {
          if (line.toLowerCase().includes(claim.toLowerCase())) {
            hits.push(`${path}:${i + 1} contains "${claim}"`);
          }
        }
      });
    }

    expect(hits, hits.join("\n")).toEqual([]);
  });

  it("has zero retired brand noun hits in source copy", () => {
    const hits: string[] = [];
    const retiredBrand = ["use", "zom", "bie"].join("");
    const retiredNoun = ["zom", "bie"].join("");
    const retiredPattern = new RegExp(`\\b(${retiredBrand}|${retiredNoun})\\b`, "i");

    for (const [path, body] of Object.entries(allMarketingSources)) {
      body.split("\n").forEach((line, i) => {
        if (retiredPattern.test(line)) {
          hits.push(`${path}:${i + 1}`);
        }
      });
    }

    expect(hits, hits.join("\n")).toEqual([]);
  });
});
