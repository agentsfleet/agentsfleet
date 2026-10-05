import { describe, expect, it } from "vitest";

import {
  ARGS_LEAF_MAX_BYTES,
  CLIP_MARK,
  clipMarked,
  compactArgs,
  firstLine,
  firstString,
  linesOf,
  mayBeClipped,
  outputLines,
  pathArg,
  scalarArg,
  workspacePath,
} from "./tool-call-text";

const LONG = "x".repeat(ARGS_LEAF_MAX_BYTES);

describe("tool-call-text", () => {
  it("treats a string at the cut length as possibly clipped, counting bytes", () => {
    expect(mayBeClipped("x".repeat(ARGS_LEAF_MAX_BYTES - 4))).toBe(false);
    expect(mayBeClipped("x".repeat(ARGS_LEAF_MAX_BYTES - 3))).toBe(true);
    // 85 three-byte characters: 255 bytes, cut at the boundary below 256.
    expect(mayBeClipped("€".repeat(85))).toBe(true);
    expect(clipMarked(LONG)).toBe(`${LONG}${CLIP_MARK}`);
    expect(clipMarked("short")).toBe("short");
  });

  it("shows a sandbox path relative to the workspace, marked if cut", () => {
    expect(workspacePath("/workspace/deploy.yaml")).toBe("deploy.yaml");
    expect(workspacePath("/etc/hosts")).toBe("/etc/hosts");
    expect(pathArg({ path: "/workspace/a.md" })).toBe("a.md");
    expect(pathArg({ path: 7 })).toBe("");
    expect(pathArg({ path: `/workspace/${LONG}` }).endsWith(CLIP_MARK)).toBe(true);
  });

  it("splits lines, a final break ending the last", () => {
    expect(linesOf("a\nb\n")).toEqual(["a", "b"]);
    expect(linesOf("a\r\nb")).toEqual(["a", "b"]);
    expect(linesOf("")).toEqual([]);
  });

  it("test_output_drops_terminal_control", () => {
    const coloured = "\u001B[32mok\u001B[0m 3 passed\n\u001B]0;title\u0007build\u001B=\n";
    expect(outputLines(coloured)).toEqual(["ok 3 passed", "build"]);
    expect(outputLines("10%\r50%\r100% done\nnext")).toEqual(["100% done", "next"]);
    // CRLF still ends a line rather than overwriting it.
    expect(outputLines("a\r\nb\r\n")).toEqual(["a", "b"]);
  });

  it("takes the first spelling present, and ids as strings or numbers", () => {
    expect(firstString({ content: "hi", text: "first" }, ["text", "content"])).toBe("first");
    expect(firstString({ content: "hi" }, ["text", "content"])).toBe("hi");
    expect(firstString({ text: 3 }, ["text"])).toBeUndefined();
    expect(scalarArg({ session_id: 7 }, "session_id")).toBe("7");
    expect(scalarArg({ session_id: "s1" }, "session_id")).toBe("s1");
    expect(scalarArg({ session_id: true }, "session_id")).toBeUndefined();
  });

  it("names a text by its first line, marked when more follows or it was cut", () => {
    expect(firstLine("one line")).toBe("one line");
    expect(firstLine("first\nsecond")).toBe(`first${CLIP_MARK}`);
    expect(firstLine("trailing\n")).toBe("trailing");
    expect(firstLine(LONG)).toBe(`${LONG}${CLIP_MARK}`);
    expect(firstLine("")).toBe("");
  });

  it("shows arguments compact and bounded", () => {
    expect(compactArgs({ app: "x" })).toBe('({"app":"x"})');
    expect(compactArgs({ note: "y".repeat(500) }).endsWith("…)")).toBe(true);
  });
});
