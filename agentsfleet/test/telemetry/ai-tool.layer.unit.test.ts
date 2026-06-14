// aiToolLayer coverage. The layer wraps @vercel/detect-agent with an
// Effect.promise + timeoutOption + catch fallback. Tests mock the
// package via bun:test mock.module so the detection, timeout, and
// rejection paths can all be exercised without env-var side effects.

import { afterEach, beforeEach, describe, expect, it, mock } from "bun:test";
import { Effect, Option } from "effect";

const detectImpl = { fn: async () => ({ isAgent: false }) as unknown };

mock.module("@vercel/detect-agent", () => ({
  determineAgent: () => detectImpl.fn(),
}));

const { aiToolLayer } = await import("../../src/services/telemetry/ai-tool.layer.ts");
const { AiTool } = await import("../../src/services/telemetry/ai-tool.service.ts");

function setDetect(fn: () => Promise<unknown>): void {
  detectImpl.fn = fn;
}

beforeEach(() => {
  setDetect(async () => ({ isAgent: false }));
});

afterEach(() => {
  setDetect(async () => ({ isAgent: false }));
});

function runLayer() {
  return Effect.runPromise(
    Effect.gen(function* () {
      return yield* AiTool;
    }).pipe(Effect.provide(aiToolLayer)),
  );
}

describe("aiToolLayer", () => {
  it("resolves to None when determineAgent reports isAgent=false", async () => {
    setDetect(async () => ({ isAgent: false }));
    const svc = await runLayer();
    expect(Option.isNone(svc.name)).toBe(true);
  });

  it("resolves to Some(normalized) when an agent is detected", async () => {
    setDetect(async () => ({ isAgent: true, agent: { name: "claude-code" } }));
    const svc = await runLayer();
    expect(Option.getOrNull(svc.name)).toBe("claude_code");
  });

  it("falls back to None when determineAgent rejects", async () => {
    setDetect(async () => {
      throw new Error("boom");
    });
    const svc = await runLayer();
    expect(Option.isNone(svc.name)).toBe(true);
  });

  it("falls back to None when determineAgent hangs past the 250ms timeout", async () => {
    setDetect(
      () =>
        new Promise(() => {
          // never resolves
        }),
    );
    const svc = await runLayer();
    expect(Option.isNone(svc.name)).toBe(true);
  }, 5_000);
});
