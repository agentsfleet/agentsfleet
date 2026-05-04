import { describe, it, expect } from "vitest";

// Spec §6 marketing tests required by docs/v2/active/M51_001 Test Specification:
// - test_marketing_hero_pillars_match: Hero.tsx must contain "Durable", "BYOK",
//   "markdown-defined" — the architecture §0 three pillars.
// - test_marketing_install_command_npm: 0 hits on `usezombie.sh/install.sh`
//   (the dead curl-pipe-bash path) and ≥1 hit on `npm install -g @usezombie/zombiectl`.
//
// Test names follow RULE TST-NAM (no milestone IDs in test names). Uses Vite
// import.meta.glob to stay browser-friendly in the vitest jsdom env.

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
    "!/src/marketing-no-legacy-pr-framing.test.ts",
    "!/src/marketing-spec.test.ts",
  ],
  { eager: true, query: "?raw", import: "default" },
);

describe("marketing hero — three architecture pillars present", () => {
  it("Hero.tsx contains 'Durable', 'BYOK', and 'markdown-defined'", () => {
    const heroFiles = Object.values(heroSource);
    expect(heroFiles, "Hero.tsx not found by import.meta.glob").toHaveLength(1);
    const body = heroFiles[0];
    expect(body, "Hero.tsx missing pillar token: Durable").toMatch(/Durable/);
    expect(body, "Hero.tsx missing pillar token: BYOK").toMatch(/BYOK/);
    expect(body, "Hero.tsx missing pillar token: markdown-defined").toMatch(
      /markdown-defined/,
    );
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
