import { describe, it, expect } from "vitest";

// Vocab guard (website twin of ui/packages/app/tests/vocab-guard.test.ts): the
// user-facing product noun is "agent". The word "agent" may appear in code
// identifiers, routes, schema names, the brand (agentsfleet), and CLI/daemon
// names — but never as a bare English product noun in *rendered copy*. This
// scan extracts user-visible text (string literals + JSX text) across the whole
// website src tree (Hero, FAQ, Pricing, Agents, CTABlock, OnboardingFlow, …)
// and fails if a multi-word phrase still contains the standalone word "agent",
// so the rename can't silently regress on the marketing site either.
//
// Sources are read with Vite import.meta.glob (?raw) — the browser-friendly
// pattern used by marketing-spec.test.ts — so this stays jsdom-safe (no node:fs).
//
// Scope is deliberately narrow to avoid false positives:
//   - only string literals ("…", '…', `…`) and same-line JSX text (>…<)
//   - only multi-word phrases (a space) — single tokens are
//     identifiers / paths / test-ids / class names / enum values, not copy
//   - comment lines are skipped
//   - brand/route/schema fragments are allowlisted
const ALLOW_FRAGMENTS = ["agentsfleet", "/agents", "agent:", "agent_", "core.agent", "agent-"];
const WORD = /\bagents?\b/i;

const SEGMENT = /"([^"]*)"|'([^']*)'|`([^`]*)`|>([^<>{}]*)</g;

const sources = import.meta.glob<string>(
  ["/src/**/*.{ts,tsx}", "!/src/**/*.test.{ts,tsx}", "!/src/**/*.spec.{ts,tsx}"],
  { eager: true, query: "?raw", import: "default" },
);

function isComment(line: string): boolean {
  const t = line.trim();
  return t.startsWith("//") || t.startsWith("*") || t.startsWith("/*");
}

describe("vocab guard — no user-facing 'agent' product noun (website)", () => {
  it("rendered copy names the agent, never the retired noun 'agent'", () => {
    const offenders: string[] = [];
    for (const [path, content] of Object.entries(sources)) {
      content.split("\n").forEach((line, i) => {
        if (isComment(line)) return;
        for (const m of line.matchAll(SEGMENT)) {
          const seg = m[1] ?? m[2] ?? m[3] ?? m[4] ?? "";
          if (!seg.includes(" ")) continue; // single token = identifier, not copy
          if (!WORD.test(seg)) continue;
          if (ALLOW_FRAGMENTS.some((f) => seg.includes(f))) continue;
          offenders.push(`${path}:${i + 1}: "${seg.trim()}"`);
        }
      });
    }
    expect(offenders, `user-facing "agent" copy found:\n${offenders.join("\n")}`).toEqual([]);
  });
});
