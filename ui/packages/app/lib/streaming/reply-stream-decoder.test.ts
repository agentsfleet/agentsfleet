import { describe, expect, it, vi } from "vitest";
import { Parser } from "htmlparser2";
import { ReplyStreamDecoder, type ReplyDelta } from "./reply-stream-decoder";

function collect(): { decoder: ReplyStreamDecoder; deltas: ReplyDelta[] } {
  const deltas: ReplyDelta[] = [];
  return { decoder: new ReplyStreamDecoder((delta) => deltas.push(delta)), deltas };
}

function text(deltas: ReplyDelta[]): { answer: string; reasoning: string } {
  return {
    answer: deltas.map((delta) => delta.answer).join(""),
    reasoning: deltas.map((delta) => delta.reasoning).join(""),
  };
}

describe("ReplyStreamDecoder", () => {
  it("publishes an answer before the model pass ends", async () => {
    let first: (() => void) | null = null;
    const arrived = new Promise<void>((resolve) => { first = resolve; });
    const deltas: ReplyDelta[] = [];
    const decoder = new ReplyStreamDecoder((delta) => {
      deltas.push(delta);
      first?.();
    });
    decoder.write("Hello");
    await arrived;
    expect(text(deltas).answer).toBe("Hello");
    decoder.write(" world");
    await decoder.finish();
    expect(text(deltas).answer).toBe("Hello world");
  });

  it("coalesces small deltas after the first paint and flushes the whole answer at close", async () => {
    const { decoder, deltas } = collect();
    decoder.write("first ");
    await vi.waitFor(() => expect(text(deltas).answer).toBe("first "));
    for (let index = 0; index < 500; index += 1) decoder.write("x");
    await decoder.finish();
    expect(text(deltas).answer).toBe(`first ${"x".repeat(500)}`);
    expect(deltas.length).toBeLessThan(20);
  });

  it("discards a buffered display update when the transport becomes incomplete", async () => {
    const { decoder, deltas } = collect();
    decoder.write("Safe ");
    await vi.waitFor(() => expect(text(deltas).answer).toBe("Safe "));
    decoder.write("buffered");
    await new Promise((resolve) => setTimeout(resolve, 0));
    decoder.markGap();
    await decoder.finish();
    expect(text(deltas).answer).toBe("Safe ");
  });

  it("separates reasoning split across transport frames from the answer", async () => {
    const { decoder, deltas } = collect();
    for (const chunk of ["<thi", "nk>Look", " here</thi", "nk>Done."]) decoder.write(chunk);
    await decoder.finish();
    expect(text(deltas)).toEqual({ reasoning: "Look here", answer: "Done." });
    expect(deltas.at(-1)?.thinking).toBe(false);
  });

  it("reveals reasoning before its closing tag arrives", async () => {
    const { decoder, deltas } = collect();
    decoder.write("<think>First long thought ");
    await vi.waitFor(() => expect(text(deltas).reasoning).toBe("First long thought "));
    expect(deltas.at(-1)?.thinking).toBe(true);
    decoder.write("continues</think>Answer");
    await decoder.finish();
    expect(text(deltas)).toEqual({ reasoning: "First long thought continues", answer: "Answer" });
    expect(deltas.at(-1)?.thinking).toBe(false);
  });

  it("uses the package parser to hide a quoted closing tag in tool JSON", async () => {
    const { decoder, deltas } = collect();
    const input = '<think>Checking <tool_call>{"name":"memory_store","arguments":{"content":"x \\" </tool_call> PRIVATE"}}</tool_call>Done.</think>Answer.';
    for (const chunk of input.match(/.{1,5}/gs) ?? []) decoder.write(chunk);
    await decoder.finish();
    expect(text(deltas)).toEqual({ reasoning: "Checking Done.", answer: "Answer." });
    expect(JSON.stringify(deltas)).not.toContain("PRIVATE");
  });

  it("hides an unfinished tool call and its arguments", async () => {
    const { decoder, deltas } = collect();
    decoder.write('<think>Checking <tool_call>{"name":"memory_store","arguments":{"content":"PRIVATE"');
    await decoder.finish();
    expect(text(deltas)).toEqual({ reasoning: "Checking ", answer: "" });
    expect(JSON.stringify(deltas)).not.toContain("PRIVATE");
  });

  it("fails closed after a tool result even if its body quotes a closing marker", async () => {
    const { decoder, deltas } = collect();
    decoder.write("<think>Checking <tool_result>echo </tool_result> PRIVATE");
    decoder.write("</tool_result> Done.</think>Answer.");
    await decoder.finish();
    expect(decoder.needsFinalReply).toBe(true);
    expect(text(deltas)).toEqual({ reasoning: "", answer: "" });
    expect(JSON.stringify(deltas)).not.toContain("PRIVATE");
  });

  it.each([
    '<tool_result name="shell" status="ok">PRIVATE</tool_result>',
    '<tool_result_begin name="shell">PRIVATE<tool_result_end>',
    '<|tool_result_begin|>PRIVATE<|tool_result_end|>',
  ])("fails closed on attributed and alternate result delimiters: %s", async (result) => {
    const { decoder, deltas } = collect();
    decoder.write(`<think>Checking ${result} done</think>Answer`);
    await decoder.finish();
    expect(text(deltas)).toEqual({ reasoning: result.startsWith("<tool_result ") ? "" : "Checking ", answer: "" });
    expect(decoder.needsFinalReply).toBe(true);
    expect(JSON.stringify(deltas)).not.toContain("PRIVATE");
  });

  it("withholds a tool result opener split between frames", async () => {
    const { decoder, deltas } = collect();
    decoder.write("Safe <tool_res");
    decoder.write("ult>PRIVATE");
    await decoder.finish();
    expect(text(deltas)).toEqual({ reasoning: "", answer: "Safe " });
    expect(JSON.stringify(deltas)).not.toContain("PRIVATE");
  });

  it("withholds a partial delimiter at the end of a reply", async () => {
    const { decoder, deltas } = collect();
    decoder.write("Literal <tool_res");
    await decoder.finish();
    expect(text(deltas).answer).toBe("Literal ");
    expect(decoder.needsFinalReply).toBe(false);
    await decoder.finish();
    decoder.write("ignored");
    expect(text(deltas).answer).toBe("Literal ");
  });

  it("streams ordinary math with an angle bracket", async () => {
    const { decoder, deltas } = collect();
    decoder.write("Compare x < y and x > z.");
    await decoder.finish();
    expect(text(deltas).answer).toBe("Compare x < y and x > z.");
    expect(decoder.needsFinalReply).toBe(false);
  });

  it("uses the durable reply for literal HTML code markup", async () => {
    const { decoder, deltas } = collect();
    decoder.write("Use `<div>` in JSX.");
    await decoder.finish();
    expect(text(deltas).answer).toBe("Use `");
    expect(decoder.needsFinalReply).toBe(true);
  });

  it.each(["</tool_result>PRIVATE", "</tool_call>PRIVATE"])(
    "never treats a stray tool closer as answer text: %s", async (input) => {
      const { decoder, deltas } = collect();
      decoder.write(input);
      await decoder.finish();
      expect(decoder.needsFinalReply).toBe(true);
      expect(JSON.stringify(deltas)).not.toContain("PRIVATE");
    },
  );

  it("recognizes a stray closer split between model deltas", async () => {
    const { decoder, deltas } = collect();
    decoder.write("</tool_res");
    decoder.write("ult>PRIVATE");
    await decoder.finish();
    expect(decoder.needsFinalReply).toBe(true);
    expect(JSON.stringify(deltas)).not.toContain("PRIVATE");
  });

  it("rejects a mismatched close inside reasoning before showing its tail", async () => {
    const { decoder, deltas } = collect();
    decoder.write("<think>Safe ");
    await vi.waitFor(() => expect(text(deltas).reasoning).toBe("Safe "));
    decoder.write("</tool_result>PRIVATE");
    await decoder.finish();
    expect(text(deltas).reasoning).toBe("Safe ");
    expect(decoder.needsFinalReply).toBe(true);
    expect(JSON.stringify(deltas)).not.toContain("PRIVATE");
  });

  it.each(["<!--PRIVATE-->", "<?PRIVATE?>"])("hides protocol markup %s", async (markup) => {
    const { decoder, deltas } = collect();
    decoder.write(`Safe ${markup} later`);
    await decoder.finish();
    expect(text(deltas).answer).toBe("Safe ");
    expect(decoder.needsFinalReply).toBe(true);
    expect(JSON.stringify(deltas)).not.toContain("PRIVATE");
  });

  it("uses the durable answer for malformed nested reasoning", async () => {
    const { decoder, deltas } = collect();
    decoder.write("<think>One<think>Two</think>Three</think>Answer");
    await decoder.finish();
    expect(text(deltas)).toEqual({ reasoning: "One", answer: "" });
    expect(decoder.needsFinalReply).toBe(true);
  });

  it("requests the durable answer if the parser throws", async () => {
    const broken = vi.spyOn(Parser.prototype, "write").mockImplementationOnce(() => { throw new Error("parser failure"); });
    try {
      const { decoder, deltas } = collect();
      decoder.write("PRIVATE");
      await decoder.finish();
      expect(decoder.needsFinalReply).toBe(true);
      expect(deltas).toEqual([]);
    } finally {
      broken.mockRestore();
    }
  });

  it("drops queued display updates after its owner disposes it", async () => {
    const { decoder, deltas } = collect();
    decoder.write("private after teardown");
    decoder.dispose();
    await decoder.finish();
    expect(deltas).toEqual([]);
  });

  it("fails closed after a transport gap and asks for the durable final reply", async () => {
    const { decoder, deltas } = collect();
    decoder.write("Safe ");
    await vi.waitFor(() => expect(text(deltas).answer).toBe("Safe "));
    decoder.markGap();
    decoder.write("PRIVATE continuation after reconnect");
    await decoder.finish();
    expect(text(deltas).answer).toBe("Safe ");
    expect(decoder.needsFinalReply).toBe(true);
  });
});
