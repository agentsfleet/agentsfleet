// initNdjsonExporter + exportSpanToNdjson coverage. Adapted from
// Supabase ndjson.unit.test.ts — usezombie's exporter is sync (writes
// via appendFileSync), so tests are plain bun:test without Effect.

import { describe, expect, it } from "bun:test";
import {
  existsSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  readdirSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import {
  exportSpanToNdjson,
  initNdjsonExporter,
} from "../../../src/services/telemetry/exporters/ndjson.ts";
import type { Tracer } from "effect";

function makeTempDir(): string {
  return mkdtempSync(path.join(tmpdir(), "agentsfleet-ndjson-test-"));
}

function endedSpan(
  name: string,
  exit: { _tag: "Success"; value: unknown } | { _tag: "Failure"; cause: unknown } = {
    _tag: "Success",
    value: undefined,
  },
  attrs: Record<string, unknown> = {},
): Tracer.Span {
  const startTime = BigInt(Date.now()) * 1_000_000n;
  const endTime = startTime + 50_000_000n;
  const attributes = new Map<string, unknown>(Object.entries(attrs));
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
    status: { _tag: "Ended", startTime, endTime, exit: exit as never },
    attributes,
    end: () => {},
    attribute: () => {},
    event: () => {},
    addLinks: () => {},
  } as unknown as Tracer.Span;
}

describe("initNdjsonExporter", () => {
  it("creates the traces directory when missing", () => {
    const dir = makeTempDir();
    try {
      const tracesDir = path.join(dir, "traces");
      initNdjsonExporter(tracesDir);
      expect(existsSync(tracesDir)).toBe(true);
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  });

  it("is idempotent when the directory already exists", () => {
    const dir = makeTempDir();
    try {
      initNdjsonExporter(dir);
      initNdjsonExporter(dir);
      expect(existsSync(dir)).toBe(true);
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  });

  it("prunes ndjson files older than the retention window", () => {
    const dir = makeTempDir();
    try {
      mkdirSync(dir, { recursive: true });
      const oldDate = new Date(Date.now() - 30 * 24 * 60 * 60 * 1000)
        .toISOString()
        .split("T")[0];
      const recentDate = new Date().toISOString().split("T")[0];
      writeFileSync(path.join(dir, `${oldDate}.ndjson`), "old\n");
      writeFileSync(path.join(dir, `${recentDate}.ndjson`), "new\n");
      writeFileSync(path.join(dir, "not-a-date.txt"), "ignore\n");
      initNdjsonExporter(dir);
      const remaining = readdirSync(dir).sort();
      expect(remaining).toContain(`${recentDate}.ndjson`);
      expect(remaining).not.toContain(`${oldDate}.ndjson`);
      expect(remaining).toContain("not-a-date.txt");
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  });

  it("ignores files whose name does not parse as a date", () => {
    const dir = makeTempDir();
    try {
      mkdirSync(dir, { recursive: true });
      writeFileSync(path.join(dir, "garbage.ndjson"), "x\n");
      initNdjsonExporter(dir);
      expect(existsSync(path.join(dir, "garbage.ndjson"))).toBe(true);
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  });

  it("never throws when readdir fails (path is a regular file)", () => {
    const dir = makeTempDir();
    try {
      const filePath = path.join(dir, "blocker");
      writeFileSync(filePath, "regular file");
      expect(() => initNdjsonExporter(filePath)).not.toThrow();
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  });
});

describe("exportSpanToNdjson", () => {
  it("writes one JSON line per ended span", () => {
    const dir = makeTempDir();
    try {
      const span = endedSpan("test-span", undefined as never, { custom: "v" });
      exportSpanToNdjson(span, dir);
      const today = new Date().toISOString().split("T")[0];
      const body = readFileSync(path.join(dir, `${today}.ndjson`), "utf8");
      const parsed = JSON.parse(body.trim());
      expect(parsed.name).toBe("test-span");
      expect(parsed.traceId).toBe("def456");
      expect(parsed.spanId).toBe("abc123");
      expect(parsed.status).toBe("ok");
      expect(parsed.attributes.custom).toBe("v");
      expect(typeof parsed.duration_ms).toBe("number");
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  });

  it("emits error_code for non-Success exit", () => {
    const dir = makeTempDir();
    try {
      const span = endedSpan("err-span", { _tag: "Failure", cause: { _tag: "Fail" } });
      exportSpanToNdjson(span, dir);
      const today = new Date().toISOString().split("T")[0];
      const body = readFileSync(path.join(dir, `${today}.ndjson`), "utf8");
      const parsed = JSON.parse(body.trim());
      expect(parsed.status).toBe("error");
      expect(parsed.error_code).toBeDefined();
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  });

  it("does nothing when the span is not Ended", () => {
    const dir = makeTempDir();
    try {
      const span = {
        ...endedSpan("not-ended"),
        status: { _tag: "Started", startTime: 0n } as never,
      } as Tracer.Span;
      exportSpanToNdjson(span, dir);
      const today = new Date().toISOString().split("T")[0];
      expect(existsSync(path.join(dir, `${today}.ndjson`))).toBe(false);
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  });

  it("swallows write errors (path is not a directory)", () => {
    const dir = makeTempDir();
    try {
      const filePath = path.join(dir, "blocker");
      writeFileSync(filePath, "regular file");
      const span = endedSpan("ok-span");
      expect(() => exportSpanToNdjson(span, filePath)).not.toThrow();
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  });
});
