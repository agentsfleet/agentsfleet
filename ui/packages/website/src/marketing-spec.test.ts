import { describe, it, expect } from "vitest";

// Hero.tsx must surface the three architecture pillars: always-on framing,
// markdown-defined nature, and the replayable event log. The earlier "Durable
// / BYOK / markdown-defined" trio was promoted out as the framing shifted
// toward the always-on event-driven story; BYOK is now a feature paragraph,
// not a hero pillar. The install-command rules are unchanged: zero hits on
// the dead `usezombie.sh/install.sh` curl-pipe path, ≥1 hit on the npm path.
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
  it("Hero.tsx contains 'Always-on', 'Markdown-defined', and 'event log'", () => {
    const heroFiles = Object.values(heroSource);
    expect(heroFiles, "Hero.tsx not found by import.meta.glob").toHaveLength(1);
    const body = heroFiles[0];
    expect(body, "Hero.tsx missing pillar token: Always-on").toMatch(/Always-on/);
    expect(body, "Hero.tsx missing pillar token: Markdown-defined").toMatch(
      /Markdown-defined/,
    );
    expect(body, "Hero.tsx missing pillar token: event log").toMatch(/event log/);
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
