import type { ToolArgs } from "@/lib/streaming/fleet-stream-tool-trace";

import { ARGS_NOT_RECORDED, TOOL_NAME } from "./tool-call-shape";
import { clipMarked, outputLines, pathArg, scalarArg, stringArg } from "./tool-call-text";

// The calls Codex folds under one "Explored" cell: the ones that look and
// change nothing. One table says both which tools fold and how each reads, so
// a tool cannot join the group without its words.

export const EXPLORE_VERB = { READ: "Read", SEARCH: "Search", LIST: "List", FETCH: "Fetch" } as const;

type LineCopy = { verb: (typeof EXPLORE_VERB)[keyof typeof EXPLORE_VERB]; target: string; scope: string | null };

/** A folded call: what it was, and for one that failed, its output. */
export type ExploreCall = { name: string; args: ToolArgs | undefined; failed: boolean; output?: string };

/**
 * One line under Explored: a verb, what it looked at in first-seen order, and
 * where. A read names each file by the shortest path that tells it from the
 * other files on its line; a failed call carries its output's first line.
 */
export type ExploreLine = Omit<LineCopy, "target"> & { targets: readonly string[]; failed: boolean; error: string | null };

const ARG = { QUERY: "query", CATEGORY: "category", URL: "url" } as const;
const MEMORY_SCOPE = "in memory";
const WEB_SCOPE = "on the web";
const THE_WEB = "the web";
const MEMORY_WORD = "memory";
const SCHEDULES = "schedules";
const RUNS_OF = "runs of";
// The spellings a schedule's id goes by: the runner verbs, and the earlier runner's tools.
const SCHEDULE_ID = ["schedule_id", "id", "job_id"] as const;
const PATH_SEPARATOR = "/";

// A Map, not an object literal: a tool named `constructor` must miss.
const EXPLORE_LINE: ReadonlyMap<string, (args: ToolArgs | undefined) => LineCopy> = new Map([
  [TOOL_NAME.FILE_READ, readLine],
  [TOOL_NAME.FILE_READ_HASHED, readLine],
  [TOOL_NAME.MEMORY_RECALL, searchLine],
  [TOOL_NAME.MEMORY_LIST, listLine],
  [TOOL_NAME.WEB_FETCH, (args) => ({ verb: EXPLORE_VERB.FETCH, target: urlTarget(args), scope: null })],
  [TOOL_NAME.WEB_SEARCH, webSearchLine],
  [TOOL_NAME.CRON_LIST, () => ({ verb: EXPLORE_VERB.LIST, target: SCHEDULES, scope: null })],
  [TOOL_NAME.CRON_RUNS, runsLine],
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
  const lines: (Omit<ExploreLine, "targets"> & { paths: Set<string> })[] = [];
  for (const { name, args, failed, output } of calls) {
    const copy = EXPLORE_LINE.get(name)?.(args);
    if (copy === undefined) continue;
    const last = lines.at(-1);
    if (copy.verb === EXPLORE_VERB.READ && !failed && last?.verb === EXPLORE_VERB.READ && !last.failed) {
      last.paths.add(copy.target);
    } else {
      const error = failed && output !== undefined ? (outputLines(output)[0] ?? null) : null;
      lines.push({ verb: copy.verb, paths: new Set([copy.target]), scope: copy.scope, failed, error });
    }
  }
  return lines.map(({ paths, ...line }) => ({
    ...line,
    targets: line.verb === EXPLORE_VERB.READ ? distinctNames([...paths]) : [...paths],
  }));
}

/** Each path by its last segment, or as many more as it takes to tell it from
 * another on the same line: `a/index.ts, b/index.ts`. */
export function distinctNames(paths: readonly string[]): string[] {
  const segmented = paths.map((path) => path.split(PATH_SEPARATOR));
  return segmented.map((segments) => {
    let shown = 1;
    while (shown < segments.length && segmented.some((other) => other !== segments && tail(other, shown) === tail(segments, shown))) shown += 1;
    return tail(segments, shown);
  });
}

function tail(segments: readonly string[], count: number): string {
  return segments.slice(-count).join(PATH_SEPARATOR);
}

function readLine(args: ToolArgs | undefined): LineCopy {
  const path = pathArg(args);
  return { verb: EXPLORE_VERB.READ, target: path.length > 0 ? path : ARGS_NOT_RECORDED, scope: null };
}

function searchLine(args: ToolArgs | undefined): LineCopy {
  const query = stringArg(args, ARG.QUERY);
  return { verb: EXPLORE_VERB.SEARCH, target: query === undefined ? ARGS_NOT_RECORDED : clipMarked(query), scope: MEMORY_SCOPE };
}

function urlTarget(args: ToolArgs | undefined): string {
  const url = stringArg(args, ARG.URL);
  return url === undefined ? ARGS_NOT_RECORDED : clipMarked(url);
}

// The provider runs web search and names its own arguments; a query, when
// the call carries one, is what it looked for.
function webSearchLine(args: ToolArgs | undefined): LineCopy {
  const query = stringArg(args, ARG.QUERY);
  return query === undefined
    ? { verb: EXPLORE_VERB.SEARCH, target: THE_WEB, scope: null }
    : { verb: EXPLORE_VERB.SEARCH, target: clipMarked(query), scope: WEB_SCOPE };
}

function runsLine(args: ToolArgs | undefined): LineCopy {
  const id = SCHEDULE_ID.map((key) => scalarArg(args, key)).find((value) => value !== undefined);
  return { verb: EXPLORE_VERB.LIST, target: id === undefined ? `${RUNS_OF} ${SCHEDULES}` : `${RUNS_OF} ${id}`, scope: null };
}

function listLine(args: ToolArgs | undefined): LineCopy {
  const category = stringArg(args, ARG.CATEGORY);
  return { verb: EXPLORE_VERB.LIST, target: category === undefined ? MEMORY_WORD : `${MEMORY_WORD} ${category}`, scope: null };
}
