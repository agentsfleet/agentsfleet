// formatSpanForDebugConsole + makeDebugConsoleExporter coverage.
// Adapted from Supabase debug-console.unit.test.ts. The exporter is
// sync and dependency-free — plain bun:test, no Effect involved.

import { describe, expect, it } from "bun:test";
import type { Tracer } from "effect";
import {
  formatSpanForDebugConsole,
  makeDebugConsoleExporter,
} from "../../../src/services/telemetry/exporters/debug-console.ts";

function endedSpan(name: string, attrs: Record<string, unknown> = {}): Tracer.Span {
  const startTime = BigInt(Date.now()) * 1_000_000n;
  const endTime = startTime + 50_000_000n;
  return {
    _tag: "Span",
    name,
    spanId: "abc123",
    traceId: "def456",
    parent: { _tag: "None" } as never,
    annotations: { _tag: "Context" } as never,
    links: [],
    sampled: true,
    kind: "internal",
    status: {
      _tag: "Ended",
      startTime,
      endTime,
      exit: { _tag: "Success", value: undefined } as never,
    },
    attributes: new Map<string, unknown>(Object.entries(attrs)),
    end: () => {},
    attribute: () => {},
    event: () => {},
    addLinks: () => {},
  } as unknown as Tracer.Span;
}

describe("formatSpanForDebugConsole", () => {
  it("returns a single-line summary with name + duration + attrs", () => {
    const span = endedSpan("test-span", { command: "login" });
    const line = formatSpanForDebugConsole(span);
    expect(line).toBeDefined();
    expect(line).toContain("test-span");
    expect(line).toContain("50ms");
    expect(line).toContain("login");
    expect(line!.endsWith("\n")).toBe(true);
  });

  it("omits the attribute object when the span has no attributes", () => {
    const span = endedSpan("bare-span");
    const line = formatSpanForDebugConsole(span)!;
    expect(line).toContain("bare-span");
    expect(line).not.toContain("{");
  });

  it("returns undefined for spans that have not ended", () => {
    const span = {
      ...endedSpan("pending"),
      status: { _tag: "Started", startTime: 0n } as never,
    } as Tracer.Span;
    expect(formatSpanForDebugConsole(span)).toBeUndefined();
  });
});

describe("makeDebugConsoleExporter", () => {
  it("invokes the writer with the formatted line for ended spans", () => {
    let captured = "";
    const exporter = makeDebugConsoleExporter((line) => {
      captured += line;
    });
    exporter(endedSpan("evt", { k: "v" }));
    expect(captured).toContain("evt");
    expect(captured).toContain("v");
  });

  it("does not invoke the writer for non-ended spans", () => {
    let calls = 0;
    const exporter = makeDebugConsoleExporter(() => {
      calls += 1;
    });
    const span = {
      ...endedSpan("pending"),
      status: { _tag: "Started", startTime: 0n } as never,
    } as Tracer.Span;
    exporter(span);
    expect(calls).toBe(0);
  });
});
