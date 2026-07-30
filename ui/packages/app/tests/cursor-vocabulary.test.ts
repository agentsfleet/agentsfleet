import { readFileSync } from "node:fs";
import { join } from "node:path";
import { describe, expect, it } from "vitest";

// One cursor vocabulary across the migrated list surfaces: requests page with
// `starting_after`, responses continue with `next_cursor`. This sweeps only
// the surfaces that have moved to that spelling — the families still spelling
// `cursor` (fleet events, workspace events, billing charges, approvals, stream
// backfill and their CLI flags) are a named follow-up and deliberately not
// covered here.

const REPO_ROOT = join(__dirname, "..", "..", "..", "..");

// The fleets list handler names the old spelling once, as the constant it
// refuses requests with — that declaration IS the refusal, not a surviving
// vocabulary, so lines referencing it are exempt.
const RETIRED_PARAM_CONST = "QUERY_CURSOR_RETIRED";

const MIGRATED_SOURCES = [
  "src/agentsfleetd/http/handlers/fleets/list.zig",
  "src/agentsfleetd/http/handlers/memory/handler.zig",
  "src/agentsfleetd/http/handlers/memory/sql.zig",
  "ui/packages/app/lib/api/fleets.ts",
  "ui/packages/app/lib/api/api_keys.ts",
  "ui/packages/app/lib/api/runners.ts",
  "ui/packages/app/lib/api/memory.ts",
  "cli/src/commands/fleet_list.ts",
  "cli/src/commands/api_key.ts",
  "cli/src/commands/memory.ts",
  "cli/src/program/cli-tree-memory.ts",
];

// Mirrors the acceptance grep: a quoted `cursor` key or parameter name, a
// `.cursor` property read, or an inline `?cursor=`/`&cursor=` query spelling.
// `next_cursor` and the keyset_cursor module never match any alternative (the
// character before `cursor` is `_` in both), so no exemption pattern is needed.
const BARE_CURSOR = /"cursor"|'cursor'|\.cursor\b|[?&]cursor=/;

describe("cursor vocabulary", () => {
  it("test_no_bare_cursor_spelling_survives", () => {
    for (const relative of MIGRATED_SOURCES) {
      const lines = readFileSync(join(REPO_ROOT, relative), "utf8").split("\n");
      lines.forEach((line, index) => {
        if (line.includes(RETIRED_PARAM_CONST)) return;
        expect(
          BARE_CURSOR.test(line),
          `${relative}:${index + 1} spells a bare cursor: ${line.trim()}`,
        ).toBe(false);
      });
    }
  });
});
