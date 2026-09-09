import { describe, expect, it } from "vitest";
import { splitReasoning } from "@/lib/events/reasoning";

describe("splitReasoning", () => {
  it("leaves a reply with no reasoning alone", () => {
    expect(splitReasoning("I'm Kimi.")).toEqual({
      reasoning: "",
      answer: "I'm Kimi.",
      thinking: false,
    });
  });

  // The shape the operator actually saw: a closed block, then the answer.
  it("separates a closed block from the answer that follows it", () => {
    const parts = splitReasoning(
      "<think>We need answer user asks name. Final only.</think>I'm Kimi, an AI assistant.",
    );
    expect(parts.answer).toBe("I'm Kimi, an AI assistant.");
    expect(parts.reasoning).toBe("We need answer user asks name. Final only.");
    expect(parts.thinking).toBe(false);
  });

  // A stream is read one chunk at a time, so an unclosed block is the normal
  // state for as long as the model is reasoning — not a malformed reply.
  it("treats an unclosed block as reasoning still arriving", () => {
    const parts = splitReasoning("<think>We need to check whether");
    expect(parts.answer).toBe("");
    expect(parts.reasoning).toBe("We need to check whether");
    expect(parts.thinking).toBe(true);
  });

  it("keeps text that arrived before the model started thinking", () => {
    const parts = splitReasoning("Sure. <think>which repo?</think>Which PR should I read?");
    expect(parts.answer).toBe("Sure. Which PR should I read?");
    expect(parts.reasoning).toBe("which repo?");
  });

  it("joins several blocks and keeps the answer contiguous", () => {
    const parts = splitReasoning("<think>one</think>Hello <think>two</think>world");
    expect(parts.reasoning).toBe("one\n\ntwo");
    expect(parts.answer).toBe("Hello world");
    expect(parts.thinking).toBe(false);
  });

  it("survives a stray closing tag with no opening one", () => {
    expect(splitReasoning("answer</think>tail").answer).toBe("answer</think>tail");
  });

  it("reports an empty reply as empty, not as thinking", () => {
    expect(splitReasoning("")).toEqual({ reasoning: "", answer: "", thinking: false });
  });
});
