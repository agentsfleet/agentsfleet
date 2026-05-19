// tracingLayer coverage. Env-driven runtime layer + span behaviour +
// exporter dispatch.

import { afterEach, beforeEach, describe, expect, it } from "bun:test";
import { Effect, Layer, Option, Tracer, Context } from "effect";
import {
  existsSync,
  mkdtempSync,
  readdirSync,
  rmSync,
} from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import { telemetryRuntimeLayer } from "../../src/services/telemetry/runtime.layer.ts";
import { tracingLayer } from "../../src/services/telemetry/tracing.layer.ts";
import { Tracing } from "../../src/services/telemetry/tracing.service.ts";

const ENV_KEYS = [
  "ZOMBIE_TELEMETRY_DISABLED",
  "DO_NOT_TRACK",
  "ZOMBIE_TELEMETRY_DEBUG",
  "ZOMBIE_STATE_DIR",
  "CI",
  "GITHUB_ACTIONS",
  "GITLAB_CI",
  "CIRCLECI",
  "JENKINS_URL",
  "BUILDKITE",
] as const;
const saved: Record<string, string | undefined> = {};
let savedIsTty: unknown;

beforeEach(() => {
  for (const k of ENV_KEYS) saved[k] = process.env[k];
  for (const k of ENV_KEYS) delete process.env[k];
  savedIsTty = process.stdout.isTTY;
  Object.defineProperty(process.stdout, "isTTY", { value: false, configurable: true });
});

afterEach(() => {
  for (const k of ENV_KEYS) {
    if (saved[k] === undefined) delete process.env[k];
    else process.env[k] = saved[k];
  }
  Object.defineProperty(process.stdout, "isTTY", { value: savedIsTty, configurable: true });
});

function makeTempDir(): string {
  return mkdtempSync(path.join(tmpdir(), "zombiectl-tracing-test-"));
}

function makeSpanOptions(
  overrides: Partial<{
    name: string;
    sampled: boolean;
    parent: Option.Option<Tracer.AnySpan>;
  }> = {},
) {
  return {
    name: overrides.name ?? "test-span",
    parent: overrides.parent ?? Option.none<Tracer.AnySpan>(),
    annotations: Context.empty() as Context.Context<never>,
    links: [] as Array<Tracer.SpanLink>,
    startTime: BigInt(Date.now()) * 1_000_000n,
    kind: "internal" as Tracer.SpanKind,
    root: false,
    sampled: overrides.sampled ?? true,
  };
}

function fullLayer() {
  return tracingLayer.pipe(Layer.provide(telemetryRuntimeLayer));
}

describe("tracingLayer — construction and consent gate", () => {
  it("builds when consent=denied (ZOMBIE_TELEMETRY_DISABLED=1); span end skips ndjson export", async () => {
    const dir = makeTempDir();
    process.env.ZOMBIE_STATE_DIR = dir;
    process.env.ZOMBIE_TELEMETRY_DISABLED = "1";
    try {
      const tracesDir = path.join(dir, "traces");
      const program = Effect.gen(function* () {
        const tracer = yield* Tracing;
        const span = tracer.span(makeSpanOptions());
        span.end(BigInt(Date.now() + 100) * 1_000_000n, { _tag: "Success", value: undefined } as never);
      }).pipe(Effect.provide(fullLayer()));
      await Effect.runPromise(program);
      const hasNdjson = existsSync(tracesDir) && readdirSync(tracesDir).some((f) => f.endsWith(".ndjson"));
      expect(hasNdjson).toBe(false);
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  });

  it("creates the traces directory when consent=granted", async () => {
    const dir = makeTempDir();
    process.env.ZOMBIE_STATE_DIR = dir;
    try {
      const program = Effect.gen(function* () {
        yield* Tracing;
      }).pipe(Effect.provide(fullLayer()));
      await Effect.runPromise(program);
      expect(existsSync(path.join(dir, "traces"))).toBe(true);
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  });
});

describe("tracingLayer — span behaviour", () => {
  it("attaches global attributes to every new span", async () => {
    const dir = makeTempDir();
    process.env.ZOMBIE_STATE_DIR = dir;
    try {
      const program = Effect.gen(function* () {
        const tracer = yield* Tracing;
        const span = tracer.span(makeSpanOptions());
        expect(span.attributes.get("schema_version")).toBe(1);
        expect(typeof span.attributes.get("device_id")).toBe("string");
        expect(typeof span.attributes.get("session_id")).toBe("string");
        expect(typeof span.attributes.get("is_first_run")).toBe("boolean");
        expect(span.attributes.get("is_tty")).toBe(false);
        expect(typeof span.attributes.get("is_ci")).toBe("boolean");
        expect(typeof span.attributes.get("os")).toBe("string");
        expect(typeof span.attributes.get("arch")).toBe("string");
        expect(typeof span.attributes.get("cli_version")).toBe("string");
      }).pipe(Effect.provide(fullLayer()));
      await Effect.runPromise(program);
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  });

  it("span end exports to NDJSON when consent=granted", async () => {
    const dir = makeTempDir();
    process.env.ZOMBIE_STATE_DIR = dir;
    try {
      const tracesDir = path.join(dir, "traces");
      const program = Effect.gen(function* () {
        const tracer = yield* Tracing;
        const span = tracer.span(makeSpanOptions());
        span.end(BigInt(Date.now() + 100) * 1_000_000n, { _tag: "Success", value: undefined } as never);
      }).pipe(Effect.provide(fullLayer()));
      await Effect.runPromise(program);
      const hasNdjson = existsSync(tracesDir) && readdirSync(tracesDir).some((f) => f.endsWith(".ndjson"));
      expect(hasNdjson).toBe(true);
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  });

  it("span end does NOT export NDJSON when ZOMBIE_TELEMETRY_DISABLED=1 (denied)", async () => {
    const dir = makeTempDir();
    process.env.ZOMBIE_STATE_DIR = dir;
    process.env.ZOMBIE_TELEMETRY_DISABLED = "1";
    try {
      const tracesDir = path.join(dir, "traces");
      const program = Effect.gen(function* () {
        const tracer = yield* Tracing;
        const span = tracer.span(makeSpanOptions());
        span.end(BigInt(Date.now() + 100) * 1_000_000n, { _tag: "Success", value: undefined } as never);
      }).pipe(Effect.provide(fullLayer()));
      await Effect.runPromise(program);
      const hasNdjson = existsSync(tracesDir) && readdirSync(tracesDir).some((f) => f.endsWith(".ndjson"));
      expect(hasNdjson).toBe(false);
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  });

  it("span end writes the debug-console line when ZOMBIE_TELEMETRY_DEBUG=1", async () => {
    const dir = makeTempDir();
    process.env.ZOMBIE_STATE_DIR = dir;
    process.env.ZOMBIE_TELEMETRY_DEBUG = "1";
    const chunks: string[] = [];
    const original = process.stderr.write.bind(process.stderr);
    process.stderr.write = ((chunk: unknown) => {
      chunks.push(String(chunk));
      return true;
    }) as typeof process.stderr.write;
    try {
      const program = Effect.gen(function* () {
        const tracer = yield* Tracing;
        const span = tracer.span(makeSpanOptions({ name: "debug-span" }));
        span.end(BigInt(Date.now() + 50) * 1_000_000n, { _tag: "Success", value: undefined } as never);
      }).pipe(Effect.provide(fullLayer()));
      await Effect.runPromise(program);
      expect(chunks.join("")).toContain("debug-span");
    } finally {
      process.stderr.write = original;
      rmSync(dir, { recursive: true, force: true });
    }
  });

  it("span end skips unsampled spans (no NDJSON export)", async () => {
    const dir = makeTempDir();
    process.env.ZOMBIE_STATE_DIR = dir;
    try {
      const tracesDir = path.join(dir, "traces");
      const program = Effect.gen(function* () {
        const tracer = yield* Tracing;
        const span = tracer.span(makeSpanOptions({ sampled: false }));
        span.end(BigInt(Date.now() + 100) * 1_000_000n, { _tag: "Success", value: undefined } as never);
      }).pipe(Effect.provide(fullLayer()));
      await Effect.runPromise(program);
      const ndjsonFiles =
        existsSync(tracesDir) && readdirSync(tracesDir).filter((f) => f.endsWith(".ndjson"));
      // Either no files at all (no other sampled span ran), or zero ndjson
      expect(ndjsonFiles === false || ndjsonFiles.length === 0).toBe(true);
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  });

  it("CI env var flips is_ci=true on the span attributes", async () => {
    const dir = makeTempDir();
    process.env.ZOMBIE_STATE_DIR = dir;
    process.env.CI = "true";
    try {
      const program = Effect.gen(function* () {
        const tracer = yield* Tracing;
        const span = tracer.span(makeSpanOptions());
        expect(span.attributes.get("is_ci")).toBe(true);
      }).pipe(Effect.provide(fullLayer()));
      await Effect.runPromise(program);
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  });
});

describe("ExportableSpan", () => {
  it("child span inherits traceId from parent span", async () => {
    const dir = makeTempDir();
    process.env.ZOMBIE_STATE_DIR = dir;
    try {
      const program = Effect.gen(function* () {
        const tracer = yield* Tracing;
        const parent = tracer.span(makeSpanOptions({ name: "parent" }));
        const child = tracer.span(
          makeSpanOptions({ name: "child", parent: Option.some(parent) }),
        );
        expect(child.traceId).toBe(parent.traceId);
      }).pipe(Effect.provide(fullLayer()));
      await Effect.runPromise(program);
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  });

  it("root span generates a 32-char hex traceId and 16-char hex spanId", async () => {
    const dir = makeTempDir();
    process.env.ZOMBIE_STATE_DIR = dir;
    const HEX_32 = /^[0-9a-f]{32}$/;
    const HEX_16 = /^[0-9a-f]{16}$/;
    try {
      const program = Effect.gen(function* () {
        const tracer = yield* Tracing;
        const span = tracer.span(makeSpanOptions());
        expect(span.traceId).toMatch(HEX_32);
        expect(span.spanId).toMatch(HEX_16);
      }).pipe(Effect.provide(fullLayer()));
      await Effect.runPromise(program);
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  });

  it("event() and addLinks() are no-ops and never throw", async () => {
    const dir = makeTempDir();
    process.env.ZOMBIE_STATE_DIR = dir;
    try {
      const program = Effect.gen(function* () {
        const tracer = yield* Tracing;
        const span = tracer.span(makeSpanOptions());
        span.event("test-event", BigInt(Date.now()) * 1_000_000n, { key: "val" });
        span.addLinks([]);
      }).pipe(Effect.provide(fullLayer()));
      await Effect.runPromise(program);
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  });

  it("attribute() persists key/value pairs onto the span", async () => {
    const dir = makeTempDir();
    process.env.ZOMBIE_STATE_DIR = dir;
    try {
      const program = Effect.gen(function* () {
        const tracer = yield* Tracing;
        const span = tracer.span(makeSpanOptions());
        span.attribute("custom", "value");
        expect(span.attributes.get("custom")).toBe("value");
      }).pipe(Effect.provide(fullLayer()));
      await Effect.runPromise(program);
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  });

  it("span end mutates status to Ended with the supplied endTime + exit", async () => {
    const dir = makeTempDir();
    process.env.ZOMBIE_STATE_DIR = dir;
    try {
      const program = Effect.gen(function* () {
        const tracer = yield* Tracing;
        const span = tracer.span(makeSpanOptions());
        const endTime = BigInt(Date.now() + 200) * 1_000_000n;
        span.end(endTime, { _tag: "Success", value: undefined } as never);
        expect(span.status._tag).toBe("Ended");
      }).pipe(Effect.provide(fullLayer()));
      await Effect.runPromise(program);
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  });

  it("span end with Failure exit still exports without throwing", async () => {
    const dir = makeTempDir();
    process.env.ZOMBIE_STATE_DIR = dir;
    try {
      const tracesDir = path.join(dir, "traces");
      const program = Effect.gen(function* () {
        const tracer = yield* Tracing;
        const span = tracer.span(makeSpanOptions({ name: "failing-span" }));
        const fakeFailure = {
          _tag: "Failure",
          cause: { _tag: "Fail", error: "boom" },
        } as never;
        span.end(BigInt(Date.now() + 50) * 1_000_000n, fakeFailure);
      }).pipe(Effect.provide(fullLayer()));
      await Effect.runPromise(program);
      const hasNdjson = existsSync(tracesDir) && readdirSync(tracesDir).some((f) => f.endsWith(".ndjson"));
      expect(hasNdjson).toBe(true);
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  });
});
