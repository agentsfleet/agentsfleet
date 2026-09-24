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

  it("omits streamed tool payloads while preserving surrounding reasoning and answer", () => {
    const parts = splitReasoning(
      '<think>Checking memory. <tool_call>{"name":"memory_recall","arguments":{"query":"private"}}</tool_call><tool_result>private memory</tool_result> Found it.</think>The answer.',
    );
    expect(parts.reasoning).toBe("Checking memory. Found it.");
    expect(parts.answer).toBe("The answer.");
    expect(parts.reasoning).not.toContain("private");
  });

  it("hides a tool call that has not closed yet", () => {
    const parts = splitReasoning('<think><tool_call>{"name":"memory_store","arguments":{"content":"private"');
    expect(parts).toEqual({ reasoning: "", answer: "", thinking: true });
  });

  it("holds back a partial tool opener until the next streamed chunk arrives", () => {
    expect(splitReasoning("<think>Checking <tool_ca").reasoning).toBe("Checking");
    expect(splitReasoning("<think>Checking <tool_call>{private").reasoning).toBe("Checking");
  });

  it("removes tool protocol text outside a reasoning block from the spoken answer", () => {
    const parts = splitReasoning('<think>Checking.</think><tool_call>{"name":"memory_store"}</tool_call>Saved.');
    expect(parts).toEqual({ reasoning: "Checking.", answer: "Saved.", thinking: false });
    expect(splitReasoning('Saved. <tool_re').answer).toBe("Saved.");
  });

  it("keeps a closing tag inside a quoted tool argument hidden", () => {
    const parts = splitReasoning('<think>Checking <tool_call>{"content":"x </tool_call> secret"}</tool_call>Done.</think>Answer.');
    expect(parts).toEqual({ reasoning: "Checking Done.", answer: "Answer.", thinking: false });
    expect(splitReasoning("<think>Checking <").reasoning).toBe("Checking");
  });

  it("ignores reasoning delimiters inside tool arguments and keeps text between results", () => {
    expect(splitReasoning('<think>Checking <tool_call>{"content":"x </think> secret"}</tool_call> Done.</think>Answer.'))
      .toEqual({ reasoning: "Checking Done.", answer: "Answer.", thinking: false });
    expect(splitReasoning("<think>Start <tool_result>one</tool_result> middle <tool_result>two</tool_result> End.</think>Answer."))
      .toEqual({ reasoning: "Start middle End.", answer: "Answer.", thinking: false });
    expect(splitReasoning("<think>Start <tool_result>x </tool_result> secret</tool_result> End.</think>Answer.").reasoning)
      .toBe("Start End.");
    expect(splitReasoning("<think>Before <tool_result>line <tool_call> literal</tool_result> After.</think>Answer."))
      .toEqual({ reasoning: "Before After.", answer: "Answer.", thinking: false });
  });

  it("survives a stray closing tag with no opening one", () => {
    expect(splitReasoning("answer</think>tail").answer).toBe("answer</think>tail");
  });

  it("reports an empty reply as empty, not as thinking", () => {
    expect(splitReasoning("")).toEqual({ reasoning: "", answer: "", thinking: false });
  });
});
