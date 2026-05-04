import { describe, it, expect } from "vitest";

// Walks the marketing src tree at test load time and asserts that no v1
// PR-validator framing has crept back into copy. Uses Vite's import.meta.glob
// to stay browser-friendly (no node:fs / @types/node dep). Spec §6
// forbidden-strings list. Test name keeps the milestone-id-free naming
// convention (RULE TST-NAM).

const FORBIDDEN_STRINGS = [
  "AI-generated PRs",
  "Automated PR delivery",
  "babysit",
  "Connect GitHub, automate PRs",
  "Validated PR delivery",
  "Run quality scoring",
  "run quality scoring",
  "Review a validated PR",
  "Queue work",
  "queued engineering work",
  "Validation before review",
  "usezombie.sh/install.sh",
] as const;

// import.meta.glob with `query: "?raw"` returns every matched file's text.
// Eager so tests don't have to await dynamic imports. Excludes test/spec
// files (they contain forbidden strings as negative assertions) and this
// file itself.
const sources = import.meta.glob<string>(
  [
    "/src/**/*.{ts,tsx,js,jsx}",
    "!/src/**/*.test.{ts,tsx}",
    "!/src/**/*.spec.{ts,tsx}",
    "!/src/marketing-no-legacy-pr-framing.test.ts",
  ],
  { eager: true, query: "?raw", import: "default" },
);

describe("marketing site copy has no v1 PR-validator framing", () => {
  it("contains zero forbidden strings across src/", () => {
    const hits: string[] = [];

    for (const [path, body] of Object.entries(sources)) {
      const lines = body.split("\n");
      for (const needle of FORBIDDEN_STRINGS) {
        lines.forEach((line, i) => {
          if (line.includes(needle)) {
            hits.push(`${path}:${i + 1} contains "${needle}"`);
          }
        });
      }
    }

    expect(hits, hits.join("\n")).toEqual([]);
  });
});
