// Behavioural + regression coverage for the top-level help tail
// (src/program/cli-tree-help.ts) and the `agent --help` lifecycle-verb
// clarification (src/program/cli-tree-agent.ts).
//
// The golden snapshot (golden-output.unit.test.ts) already pins the help
// byte-for-byte, but a snapshot only screams "something moved" — it does
// not name the invariant that broke. These tests assert the guarantees the
// help DX work introduced, so a future edit that breaks one fails with a
// meaningful message instead of an opaque diff:
//   1. the 80-column width budget holds at the source, not just the fixture
//   2. the help tail points at the configuration docs (single source of truth)
// Plus: `agent --help` spells out that the lifecycle verbs are top-level.

import { test, expect } from "bun:test";

import type { Command } from "commander";

import { helpTail } from "../src/program/cli-tree-help.ts";
import { buildProgram } from "../src/program/cli-tree.ts";
import { makeSpyTree } from "./helpers-cli-tree.ts";

const HELP_WIDTH = 80;

// Render a subcommand's full --help body (including addHelpText hooks) by
// capturing outputHelp(). outputHelp() writes through configureOutput and,
// unlike runCli(["agent","--help"]), never calls process.exit — so the
// assertion sees real text instead of an empty buffer the runner swallows.
function renderCommandHelp(...path: string[]): string {
  const { handlers } = makeSpyTree();
  const program = buildProgram({ handlers, version: "0.0.0-test", state: { exitCode: 0 } });
  let command: Command = program;
  for (const name of path) {
    const next = command.commands.find((c) => c.name() === name);
    if (!next) throw new Error(`subcommand not found: ${path.join(" ")}`);
    command = next;
  }
  let text = "";
  command.configureOutput({ writeOut: (s) => { text += s; }, writeErr: () => {} });
  command.outputHelp();
  return text;
}

function tailLines(): string[] {
  return helpTail().split("\n");
}

// ── Invariant: width budget ──────────────────────────────────────────────

test("every help-tail line stays within the 80-column budget", () => {
  const overflow = tailLines().filter((line) => line.length > HELP_WIDTH);
  expect(overflow).toEqual([]);
});

// ── Behaviour: env help points at the docs, not an inline table ──────────

test("help tail points to the configuration docs (no inline env matrix)", () => {
  const tail = helpTail();
  expect(tail).toContain("Environment variables:");
  expect(tail).toContain("https://docs.agentsfleet.net/cli/configuration");
  // The inline env-var rows were retired in favour of the docs reference —
  // guard against a regression that re-inlines the matrix here.
  for (const name of ["AGENTSFLEET_TELEMETRY_POSTHOG_HOST", "DO_NOT_TRACK"]) {
    expect(tail).not.toContain(name);
  }
});

// ── Behaviour: `agent --help` explains the top-level verbs (finding 1) ───

test("agent --help clarifies the lifecycle verbs are top-level commands", () => {
  const text = renderCommandHelp("agent");
  expect(text).toContain("top-level commands");
  // Names at least one verb a user would otherwise try as `agent <verb>`.
  expect(text).toContain("steer");
  // Points back to the full guide.
  expect(text).toContain("agentsfleet --help");
});
