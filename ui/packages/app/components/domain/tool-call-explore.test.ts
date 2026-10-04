import { describe, expect, it } from "vitest";

import { TOOL_NAME } from "./tool-call-copy";
import { EXPLORE_TOOLS, EXPLORE_VERB, exploreLines } from "./tool-call-explore";

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
      { verb: EXPLORE_VERB.READ, targets: ["a.md", "b.md"], scope: null, failed: false },
      { verb: EXPLORE_VERB.SEARCH, targets: ["deploy window"], scope: "in memory", failed: false },
      { verb: EXPLORE_VERB.LIST, targets: ["memory ops"], scope: null, failed: false },
      { verb: EXPLORE_VERB.LIST, targets: ["memory"], scope: null, failed: false },
      { verb: EXPLORE_VERB.SEARCH, targets: [""], scope: "in memory", failed: false },
    ]);
  });

  it("keeps a failed read on a line of its own", () => {
    const lines = exploreLines([read("a.md"), read("b.md", true), read("c.md")]);
    expect(shown(lines).map((line) => [line.targets, line.failed])).toEqual([[["a.md"], false], [["b.md"], true], [["c.md"], false]]);
  });

  it("adds no line for a call outside the explore set, inherited names included", () => {
    expect(exploreLines([{ name: TOOL_NAME.FILE_WRITE, args: undefined, failed: false }])).toEqual([]);
    expect(exploreLines([{ name: "constructor", args: undefined, failed: false }])).toEqual([]);
  });

  it("folds exactly the tools that look and change nothing", () => {
    expect([...EXPLORE_TOOLS].sort()).toEqual(
      [TOOL_NAME.FILE_READ, TOOL_NAME.FILE_READ_HASHED, TOOL_NAME.MEMORY_LIST, TOOL_NAME.MEMORY_RECALL].sort(),
    );
  });
});
