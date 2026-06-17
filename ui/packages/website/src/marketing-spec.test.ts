import { describe, it, expect } from "vitest";
import {
  FORBIDDEN_MARKETING_CLAIMS,
  HERO_HEADLINE,
  HERO_LEDE_PARTS,
  PILLAR_TOKENS,
} from "./lib/marketing-copy";

/*
 * Hero.tsx surfaces the approved website refresh language: the LIVE pulse
 * eyebrow, the resident-engineer wedge, and the replayable-log architecture
 * pillar.
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
