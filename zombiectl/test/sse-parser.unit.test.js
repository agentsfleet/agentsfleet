import { describe, test, expect } from "bun:test";
import { parseSseBuffer } from "../src/lib/sse-parser.js";

describe("parseSseBuffer", () => {
  test("parses single tool_use event", () => {
    const buf = 'event: tool_use\ndata: {"id":"tu_01","name":"read_file","input":{"path":"go.mod"}}\n\n';
    const { events, remainder } = parseSseBuffer(buf);
    expect(events).toHaveLength(1);
    expect(events[0].type).toBe("tool_use");
    expect(events[0].data.name).toBe("read_file");
    expect(events[0].data.input.path).toBe("go.mod");
    expect(remainder).toBe("");
  });

  test("parses multiple events", () => {
    const buf = 'event: text_delta\ndata: {"text":"hello"}\n\nevent: done\ndata: {"usage":{"total_tokens":100}}\n\n';
    const result = parseSseBuffer(buf);
    expect(result.events).toHaveLength(2);
    expect(result.events[0].type).toBe("text_delta");
    expect(result.events[0].data.text).toBe("hello");
    expect(result.events[1].type).toBe("done");
    expect(result.events[1].data.usage.total_tokens).toBe(100);
    expect(result.remainder).toBe("");
  });

  test("returns remainder for incomplete frame", () => {
    const buf = 'event: text_delta\ndata: {"text":"partial';
    const { events, remainder } = parseSseBuffer(buf);
    expect(events).toHaveLength(0);
    expect(remainder).toBe(buf);
  });

  test("skips heartbeat comments", () => {
    const buf = ': heartbeat\n\nevent: done\ndata: {"ok":true}\n\n';
    const { events } = parseSseBuffer(buf);
    // heartbeat-only frame produces null (no data line), only done event returned
    expect(events).toHaveLength(1);
    expect(events[0].type).toBe("done");
  });

  test("handles empty buffer", () => {
    const { events, remainder } = parseSseBuffer("");
    expect(events).toHaveLength(0);
    expect(remainder).toBe("");
  });

  test("parses error event", () => {
    const buf = 'event: error\ndata: {"message":"provider timeout"}\n\n';
    const { events } = parseSseBuffer(buf);
    expect(events).toHaveLength(1);
    expect(events[0].type).toBe("error");
    expect(events[0].data.message).toBe("provider timeout");
  });
});
