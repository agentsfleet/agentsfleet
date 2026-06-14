// analyticsLayer ai_tool=none coverage filler.
//
// In the primary analytics.layer test the real @vercel/detect-agent runs
// and (under this CLI's own agent env) returns isAgent:true, so
// `aiTool.name` is Option.some and the base-property `ai_tool` resolves
// through the Option.match onSome arm. The onNone arm
// (analytics.layer.ts:92-94) — which itself branches on isCi / isTty —
// is therefore never invoked there. Mock detect-agent to report no agent
// so aiToolLayer yields a None tool and the onNone closure is exercised.
//
// posthog-node is stubbed (network-free) exactly as in
// analytics.layer.unit.test.ts so capture() observes the resolved
// base properties in-process.

import { afterEach, beforeEach, describe, expect, it, mock } from "bun:test";
import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import pathMod from "node:path";

interface CapturedEvent {
  event: string;
  distinctId: string;
  properties?: Record<string, unknown>;
  groups?: Record<string, string>;
}

const captured: CapturedEvent[] = [];

mock.module("posthog-node", () => ({
  PostHog: class PostHogStubClass {
    constructor(_key: string, _opts: Record<string, unknown>) {}
    capture(evt: CapturedEvent): void {
      captured.push(evt);
    }
    identify(): void {}
    alias(): void {}
    groupIdentify(): void {}
    async _shutdown(_timeoutMs?: number): Promise<void> {}
  },
}));

// Force the no-agent branch so AiTool.name is Option.none → analytics
// base-property ai_tool routes through the onNone arm.
mock.module("@vercel/detect-agent", () => ({
  determineAgent: async () => ({ isAgent: false }),
}));

const { analyticsLayer } = await import(
  "../../src/services/telemetry/analytics.layer.ts"
);
const { Analytics } = await import(
  "../../src/services/telemetry/analytics.service.ts"
);
const { cliConfigLayer } = await import("../../src/services/config.ts");
const { Effect, Layer } = await import("effect");

const ENV_KEYS = [
  "ZOMBIE_TELEMETRY_POSTHOG_KEY",
  "ZOMBIE_TELEMETRY_POSTHOG_HOST",
  "ZOMBIE_STATE_DIR",
  "ZOMBIE_TELEMETRY_DISABLED",
  "DO_NOT_TRACK",
  "CI",
  "GITHUB_ACTIONS",
  "GITLAB_CI",
  "CIRCLECI",
  "JENKINS_URL",
  "BUILDKITE",
] as const;
const saved: Record<string, string | undefined> = {};
let tmpDir: string | undefined;

beforeEach(() => {
  for (const k of ENV_KEYS) saved[k] = process.env[k];
  for (const k of ENV_KEYS) delete process.env[k];
  captured.length = 0;
  tmpDir = mkdtempSync(pathMod.join(tmpdir(), "agentsfleet-aitool-none-"));
  process.env.ZOMBIE_STATE_DIR = tmpDir;
});

afterEach(() => {
  if (tmpDir !== undefined) rmSync(tmpDir, { recursive: true, force: true });
  tmpDir = undefined;
  for (const k of ENV_KEYS) {
    if (saved[k] === undefined) delete process.env[k];
    else process.env[k] = saved[k];
  }
});

function getAnalytics() {
  return Effect.gen(function* () {
    return yield* Analytics;
  }).pipe(Effect.provide(Layer.provide(analyticsLayer, cliConfigLayer)));
}

describe("analyticsLayer — ai_tool onNone arm", () => {
  it("resolves ai_tool via the onNone branch when no agent is detected", async () => {
    const program = Effect.scoped(
      Effect.gen(function* () {
        const svc = yield* getAnalytics();
        yield* svc.capture("evt");
      }),
    );
    await Effect.runPromise(program);
    expect(captured).toHaveLength(1);
    // onNone returns "ci" | undefined | "unknown_non_interactive" depending
    // on isCi/isTty; under the network-free non-TTY test runner (CI vars
    // stripped above) it lands on "unknown_non_interactive". Either way the
    // closure ran — assert the property is not a real agent name.
    const aiTool = captured[0]?.properties?.ai_tool;
    expect(aiTool === undefined || typeof aiTool === "string").toBe(true);
    if (typeof aiTool === "string") {
      expect(["ci", "unknown_non_interactive"]).toContain(aiTool);
    }
  });
});
