import { describe, it, expect } from "vitest";

/*
 * Hero.tsx surfaces the W2 design language: the LIVE pulse eyebrow,
 * the operational-restraint voice ("the daemon already knows why"),
 * and the replayable-log architecture pillar.
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

describe("marketing hero — W2 architecture pillars present", () => {
  it("Hero.tsx contains 'wake.on.event', 'long-lived runtime', and 'replayable log'", () => {
    const heroFiles = Object.values(heroSource);
    expect(heroFiles, "Hero.tsx not found by import.meta.glob").toHaveLength(1);
    const body = heroFiles[0];
    expect(body, "Hero.tsx missing pillar token: wake.on.event").toMatch(/wake\.on\.event/);
    expect(body, "Hero.tsx missing pillar token: long-lived runtime").toMatch(
      /long-lived runtime/,
    );
    expect(body, "Hero.tsx missing pillar token: replayable log").toMatch(/replayable log/);
  });
});

describe("marketing install command — npm path, no curl-pipe-bash", () => {
  it("zero hits on the dead `usezombie.sh/install.sh` path across src/", () => {
    const hits: string[] = [];
    for (const [path, body] of Object.entries(allMarketingSources)) {
      body.split("\n").forEach((line, i) => {
        if (line.includes("usezombie.sh/install.sh")) {
          hits.push(`${path}:${i + 1}`);
        }
      });
    }
    expect(hits, hits.join("\n")).toEqual([]);
  });

  it("at least one hit on `npm install -g @usezombie/zombiectl` across src/", () => {
    const hits: string[] = [];
    for (const [path, body] of Object.entries(allMarketingSources)) {
      body.split("\n").forEach((line, i) => {
        if (line.includes("npm install -g @usezombie/zombiectl")) {
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
