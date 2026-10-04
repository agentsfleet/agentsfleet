import type { ToolArgs } from "@/lib/streaming/fleet-stream-tool-trace";

import { TOOL_NAME, pathArg, stringArg } from "./tool-call-copy";

// The calls Codex folds under one "Explored" cell: the ones that look and
// change nothing. One table says both which tools fold and how each reads, so
// a tool cannot join the group without its words.

export const EXPLORE_VERB = { READ: "Read", SEARCH: "Search", LIST: "List" } as const;

type LineCopy = { verb: (typeof EXPLORE_VERB)[keyof typeof EXPLORE_VERB]; target: string; scope: string | null };

export type ExploreCall = { name: string; args: ToolArgs | undefined; failed: boolean };

/** One line under Explored: a verb, what it looked at in first-seen order,
 * and where. */
export type ExploreLine = Omit<LineCopy, "target"> & { targets: ReadonlySet<string>; failed: boolean };

const ARG = { QUERY: "query", CATEGORY: "category" } as const;
const MEMORY_SCOPE = "in memory";
const MEMORY_WORD = "memory";

// A Map, not an object literal: a tool named `constructor` must miss.
const EXPLORE_LINE: ReadonlyMap<string, (args: ToolArgs | undefined) => LineCopy> = new Map([
  [TOOL_NAME.FILE_READ, readLine],
  [TOOL_NAME.FILE_READ_HASHED, readLine],
  [TOOL_NAME.MEMORY_RECALL, (args) => ({ verb: EXPLORE_VERB.SEARCH, target: stringArg(args, ARG.QUERY) ?? "", scope: MEMORY_SCOPE })],
  [TOOL_NAME.MEMORY_LIST, listLine],
]);

/** The tools Explored folds, for the reply's group map. */
export const EXPLORE_TOOLS: readonly string[] = [...EXPLORE_LINE.keys()];

/**
 * Explored's lines, as Codex folds them: consecutive reads that did not fail
 * merge into one line naming each file once; a search or a listing is a line
 * of its own, and a failed call keeps its line. A call outside the table adds
 * none, though the group map never sends one here.
 */
export function exploreLines(calls: readonly ExploreCall[]): ExploreLine[] {
  const lines: (ExploreLine & { targets: Set<string> })[] = [];
  for (const { name, args, failed } of calls) {
    const copy = EXPLORE_LINE.get(name)?.(args);
    if (copy === undefined) continue;
    const last = lines.at(-1);
    if (copy.verb === EXPLORE_VERB.READ && !failed && last?.verb === EXPLORE_VERB.READ && !last.failed) {
      last.targets.add(copy.target);
    } else {
      lines.push({ verb: copy.verb, targets: new Set([copy.target]), scope: copy.scope, failed });
    }
  }
  return lines;
}

function readLine(args: ToolArgs | undefined): LineCopy {
  const path = pathArg(args);
  return { verb: EXPLORE_VERB.READ, target: path.slice(path.lastIndexOf("/") + 1), scope: null };
}

function listLine(args: ToolArgs | undefined): LineCopy {
  const category = stringArg(args, ARG.CATEGORY);
  return { verb: EXPLORE_VERB.LIST, target: category === undefined ? MEMORY_WORD : `${MEMORY_WORD} ${category}`, scope: null };
}
