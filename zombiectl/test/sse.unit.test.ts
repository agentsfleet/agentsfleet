import { describe, expect, it } from "bun:test";
import { parseSseFrame } from "../src/lib/sse.ts";

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
    const data = ev?.data as { status?: string } | undefined;
    expect(data?.status).toBe("processed");
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
    const data = ev?.data as { event_id?: string } | undefined;
    expect(data?.event_id).toBe("x");
  });

  it("handles a `data:` line with no space after the colon", () => {
    // Per SSE spec the space after `data:` is optional. The no-space
    // branch trimStart()s the remainder, so `data:hi` yields `hi`.
    const ev = parseSseFrame("event: raw\ndata:hello");
    expect(ev).toEqual({ id: null, type: "raw", data: "hello" });
  });

  it("preserves a `data:` payload that has no leading space", () => {
    // A spaceless `data:` line whose first char is non-space takes the
    // no-space branch directly (line 128); trimStart() is a no-op here.
    const ev = parseSseFrame("event: raw\ndata:tight");
    expect(ev).toEqual({ id: null, type: "raw", data: "tight" });
  });

  it("parses JSON delivered through the spaceless `data:` branch", () => {
    const ev = parseSseFrame("event: chunk\ndata:{\"event_id\":\"z\",\"text\":\"q\"}");
    expect(ev?.type).toBe("chunk");
    const data = ev?.data as { event_id?: string; text?: string } | undefined;
    expect(data?.event_id).toBe("z");
    expect(data?.text).toBe("q");
  });

  it("joins a spaceless `data:` continuation onto a prior data line", () => {
    // Exercises the `data.length > 0` true arm of the no-space branch:
    // a `data: ` line seeds content, then `data:` (no space) appends.
    const frame = "event: raw\ndata: first\ndata:second";
    const ev = parseSseFrame(frame);
    expect(ev?.type).toBe("raw");
    expect(ev?.data).toBe("first\nsecond");
  });
});
