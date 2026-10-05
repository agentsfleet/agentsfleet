import { describe, expect, it } from "vitest";

import { ARGS_NOT_RECORDED, TOOL_NAME } from "./tool-call-shape";
import { EXPLORE_TOOLS, EXPLORE_VERB, distinctNames, exploreLines } from "./tool-call-explore";

const read = (path: string, failed = false) => ({ name: TOOL_NAME.FILE_READ, args: { path }, failed });
const shown = (lines: ReturnType<typeof exploreLines>) => lines.map((line) => ({ ...line, targets: [...line.targets] }));

describe("exploreLines", () => {
  it("merges consecutive reads, naming each file once, and gives a search its own line", () => {
    const lines = shown(exploreLines([
      read("/workspace/docs/a.md"),
      { name: TOOL_NAME.FILE_READ_HASHED, args: { path: "/workspace/b.md" }, failed: false },
      read("/workspace/docs/a.md"),
      { name: TOOL_NAME.MEMORY_RECALL, args: { query: "deploy window" }, failed: false },
      { name: TOOL_NAME.MEMORY_LIST, args: { category: "ops" }, failed: false },
      { name: TOOL_NAME.MEMORY_LIST, args: undefined, failed: false },
      { name: TOOL_NAME.MEMORY_RECALL, args: undefined, failed: false },
    ]));
    expect(lines).toEqual([
      { verb: EXPLORE_VERB.READ, targets: ["a.md", "b.md"], scope: null, failed: false, error: null },
      { verb: EXPLORE_VERB.SEARCH, targets: ["deploy window"], scope: "in memory", failed: false, error: null },
      { verb: EXPLORE_VERB.LIST, targets: ["memory ops"], scope: null, failed: false, error: null },
      { verb: EXPLORE_VERB.LIST, targets: ["memory"], scope: null, failed: false, error: null },
      // A search whose arguments the runner did not keep says so, never "".
      { verb: EXPLORE_VERB.SEARCH, targets: [ARGS_NOT_RECORDED], scope: "in memory", failed: false, error: null },
    ]);
  });

  it("keeps a failed read on a line of its own", () => {
    const lines = exploreLines([read("a.md"), read("b.md", true), read("c.md")]);
    expect(shown(lines).map((line) => [line.targets, line.failed])).toEqual([[["a.md"], false], [["b.md"], true], [["c.md"], false]]);
  });

  it("tells same-named files apart by as much of their path as it takes", () => {
    const [line] = exploreLines([read("/workspace/src/a/index.ts"), read("/workspace/src/b/index.ts"), read("/workspace/README.md")]);
    expect(line?.targets).toEqual(["a/index.ts", "b/index.ts", "README.md"]);
    expect(distinctNames(["x/y/z.ts", "w/y/z.ts", "z.ts"])).toEqual(["x/y/z.ts", "w/y/z.ts", "z.ts"]);
  });

  it("gives a failed call its first output line, and a read with no arguments says so", () => {
    const lines = exploreLines([
      { ...read("secret.md", true), output: "path escapes the workspace\nmore" },
      { name: TOOL_NAME.FILE_READ, args: undefined, failed: false },
      { ...read("gone.md", true) },
      { ...read("blank.md", true), output: "" },
    ]);
    expect(lines.map((line) => [line.targets, line.error])).toEqual([
      [["secret.md"], "path escapes the workspace"],
      [[ARGS_NOT_RECORDED], null],
      [["gone.md"], null],
      [["blank.md"], null],
    ]);
  });

  it("adds no line for a call outside the explore set, inherited names included", () => {
    expect(exploreLines([{ name: TOOL_NAME.FILE_WRITE, args: undefined, failed: false }])).toEqual([]);
    expect(exploreLines([{ name: "constructor", args: undefined, failed: false }])).toEqual([]);
  });

  it("folds exactly the tools that look and change nothing", () => {
    expect([...EXPLORE_TOOLS].sort()).toEqual([
      TOOL_NAME.FILE_READ, TOOL_NAME.FILE_READ_HASHED, TOOL_NAME.MEMORY_LIST, TOOL_NAME.MEMORY_RECALL,
      TOOL_NAME.WEB_FETCH, TOOL_NAME.WEB_SEARCH, TOOL_NAME.CRON_LIST, TOOL_NAME.CRON_RUNS,
    ].sort());
  });

  it("reads the runner's web and schedule lookups as looks", () => {
    const lines = exploreLines([
      { name: TOOL_NAME.WEB_FETCH, args: { url: "https://docs.example/ops" }, failed: false },
      { name: TOOL_NAME.WEB_FETCH, args: undefined, failed: false },
      { name: TOOL_NAME.WEB_SEARCH, args: { query: "dragonfly cluster" }, failed: false },
      { name: TOOL_NAME.WEB_SEARCH, args: undefined, failed: false },
      { name: TOOL_NAME.CRON_LIST, args: undefined, failed: false },
      { name: TOOL_NAME.CRON_RUNS, args: { schedule_id: "sch_1" }, failed: false },
      { name: TOOL_NAME.CRON_RUNS, args: { limit: 5 }, failed: false },
    ]);
    expect(lines.map((line) => `${line.verb} ${line.targets.join(", ")}${line.scope === null ? "" : ` ${line.scope}`}`)).toEqual([
      "Fetch https://docs.example/ops",
      `Fetch ${ARGS_NOT_RECORDED}`,
      "Search dragonfly cluster on the web",
      "Search the web",
      "List schedules",
      "List runs of sch_1",
      "List runs of schedules",
    ]);
  });
});
