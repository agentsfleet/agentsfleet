import { describe, expect, it } from "bun:test";
import { parseSseFrame } from "../src/lib/sse.js";

describe("parseSseFrame", () => {
  it("parses a typical M42 chunk frame", () => {
    const frame = "id: 7\nevent: chunk\ndata: {\"event_id\":\"x\",\"text\":\"hello\"}";
    const ev = parseSseFrame(frame);
    expect(ev).toEqual({
      id: "7",
      type: "chunk",
      data: { event_id: "x", text: "hello" },
    });
  });

  it("returns null when no data line is present", () => {
    expect(parseSseFrame("id: 1\nevent: ping")).toBeNull();
  });

  it("ignores comment lines", () => {
    const ev = parseSseFrame(": heartbeat\nid: 0\nevent: event_complete\ndata: {\"event_id\":\"a\",\"status\":\"processed\"}");
    expect(ev?.type).toBe("event_complete");
    expect(ev?.data?.status).toBe("processed");
  });

  it("falls back to raw data when JSON.parse fails", () => {
    const ev = parseSseFrame("event: raw\ndata: not-json");
    expect(ev).toEqual({ id: null, type: "raw", data: "not-json" });
  });

  it("joins multiple data lines with a newline (SSE spec)", () => {
    // Per SSE spec, repeated `data:` lines in one frame are concatenated
    // by `\n` before delivery. Lines without the data: prefix are ignored.
    const frame = "event: raw\ndata: line one\ndata: line two";
    const ev = parseSseFrame(frame);
    expect(ev?.type).toBe("raw");
    expect(ev?.data).toBe("line one\nline two");
  });

  it("strips a trailing CR before parsing (handles CRLF event streams)", () => {
    const frame = "id: 5\r\nevent: chunk\r\ndata: {\"event_id\":\"x\"}";
    const ev = parseSseFrame(frame);
    expect(ev?.type).toBe("chunk");
    expect(ev?.data?.event_id).toBe("x");
  });
});
