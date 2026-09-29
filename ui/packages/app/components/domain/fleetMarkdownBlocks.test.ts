import { describe, expect, it } from "vitest";

import { StreamingBlocks, type ParsedTail } from "./fleetMarkdownBlocks";

// Four one-letter paragraphs: blocks start at 0, 3, 6 and 9.
const FOUR_BLOCKS = "a\n\nb\n\nc\n\nd";
const STARTS = [0, 3, 6, 9];
// The two blocks a parse of FOUR_BLOCKS proves finished, and where the held tail begins.
const PROVEN = ["a\n\n", "b\n\n"];
const HELD_TAIL = "c\n\nd";
// An answer that does not extend the one before it.
const REPLACED = "x\n\ny";

function parsed(starts: readonly number[]): ParsedTail {
  return { children: starts.map((offset) => ({ position: { start: { offset } } })) };
}

describe("StreamingBlocks", () => {
  it("promotes every block but the last two once the tail's parse proves them", () => {
    const blocks = new StreamingBlocks();
    const first = blocks.view(FOUR_BLOCKS);
    expect(first.finished).toEqual([]);
    expect(first.tail).toBe(FOUR_BLOCKS);
    first.readTail(parsed(STARTS));

    const next = blocks.view(`${FOUR_BLOCKS}e`);
    expect(next.finished).toEqual(PROVEN);
    expect(next.tail).toBe(`${HELD_TAIL}e`);
  });

  it("hands back the same finished blocks, string for string, on every later flush", () => {
    const blocks = new StreamingBlocks();
    blocks.view(FOUR_BLOCKS).readTail(parsed(STARTS));
    const promoted = blocks.view(`${FOUR_BLOCKS}e`).finished;
    const later = blocks.view(`${FOUR_BLOCKS}ef`).finished;
    expect(later).toBe(promoted);
  });

  it("measures the next tail's offsets from where that tail starts", () => {
    const blocks = new StreamingBlocks();
    blocks.view(FOUR_BLOCKS).readTail(parsed(STARTS));
    const longer = `${FOUR_BLOCKS}\n\ne\n\nf`;
    // The held tail "c\n\nd\n\ne\n\nf" parses to blocks at 0, 3, 6 and 9 of itself.
    blocks.view(longer).readTail(parsed(STARTS));
    expect(blocks.view(`${longer}g`).finished).toEqual([...PROVEN, "c\n\n", "d\n\n"]);
  });

  it("starts over when the answer is replaced rather than extended", () => {
    const blocks = new StreamingBlocks();
    blocks.view(FOUR_BLOCKS).readTail(parsed(STARTS));
    blocks.view(`${FOUR_BLOCKS}e`);
    const replaced = blocks.view(REPLACED);
    expect(replaced.finished).toEqual([]);
    expect(replaced.tail).toBe(REPLACED);
  });

  it("keeps a parse of text this flush does not hold, for a flush that does", () => {
    const blocks = new StreamingBlocks();
    blocks.view(FOUR_BLOCKS).readTail(parsed(STARTS));
    const shorter = blocks.view(FOUR_BLOCKS.slice(0, 4));
    expect(shorter.finished).toEqual([]);
    expect(blocks.view(`${FOUR_BLOCKS}e`).finished).toEqual(PROVEN);
  });

  it("ignores a parse of a tail that an earlier promotion already moved", () => {
    const blocks = new StreamingBlocks();
    const first = blocks.view(FOUR_BLOCKS);
    const sameFlush = blocks.view(FOUR_BLOCKS);
    first.readTail(parsed(STARTS));
    blocks.view(`${FOUR_BLOCKS}e`);
    sameFlush.readTail(parsed(STARTS));
    expect(blocks.view(`${FOUR_BLOCKS}ef`).finished).toEqual(PROVEN);
  });

  it("proves nothing from fewer than three blocks, or from a node with no offset", () => {
    const blocks = new StreamingBlocks();
    blocks.view(FOUR_BLOCKS).readTail(parsed([0, 3]));
    expect(blocks.view(`${FOUR_BLOCKS}e`).finished).toEqual([]);
    blocks.view(FOUR_BLOCKS).readTail({ children: [{}, {}, {}] });
    expect(blocks.view(`${FOUR_BLOCKS}e`).finished).toEqual([]);
  });
});
