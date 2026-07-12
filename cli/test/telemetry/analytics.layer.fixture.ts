import { afterEach, beforeEach, mock } from "bun:test";
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";

const POSTHOG_MODULE = "posthog-node";
const STATE_DIRECTORY_PREFIX = "agentsfleet-analytics-test-";
const TELEMETRY_FILE_NAME = "telemetry.json";
const STDOUT_IS_TTY_PROPERTY = "isTTY";
const ENV_KEYS = [
  "AGENTSFLEET_TELEMETRY_POSTHOG_KEY",
  "AGENTSFLEET_TELEMETRY_POSTHOG_HOST",
  "AGENTSFLEET_STATE_DIR",
  "AGENTSFLEET_TELEMETRY_DISABLED",
  "DO_NOT_TRACK",
  "AGENTSFLEET_TELEMETRY_DEBUG",
  "CI",
  "GITHUB_ACTIONS",
  "GITLAB_CI",
  "CIRCLECI",
  "JENKINS_URL",
  "BUILDKITE",
  "AI_AGENT",
  "CODEX_SANDBOX",
  "CODEX_CI",
  "CODEX_THREAD_ID",
  "CURSOR_TRACE_ID",
  "CURSOR_AGENT",
  "CURSOR_EXTENSION_HOST_ROLE",
  "GEMINI_CLI",
  "ANTIGRAVITY_AGENT",
  "AUGMENT_AGENT",
  "OPENCODE_CLIENT",
  "CLAUDECODE",
  "CLAUDE_CODE",
  "CLAUDE_CODE_IS_COWORK",
  "REPL_ID",
  "COPILOT_MODEL",
  "COPILOT_ALLOW_ALL",
  "COPILOT_GITHUB_TOKEN",
] as const;

interface CapturedEvent {
  event: string;
  distinctId: string;
  properties?: Record<string, unknown>;
  groups?: Record<string, string>;
}

class AnalyticsTestHarness {
  readonly captured: CapturedEvent[] = [];
  readonly identified: Array<{
    distinctId: string;
    properties?: Record<string, unknown>;
  }> = [];
  readonly aliased: Array<{ distinctId: string; alias: string }> = [];
  readonly groupIdentified: Array<{
    groupType: string;
    groupKey: string;
    distinctId: string;
    properties?: Record<string, unknown>;
  }> = [];
  shutdownCalls = 0;
  flushCalls = 0;
  flushError: Error | undefined;
  options: Record<string, unknown> | undefined;
  #saved: Record<string, string | undefined> = {};
  #tmpDir: string | undefined;

  installHooks(): void {
    beforeEach(() => this.#reset());
    afterEach(() => this.#restore());
  }

  forceStdoutIsTty(value: boolean): () => void {
    const original = process.stdout.isTTY;
    Object.defineProperty(process.stdout, STDOUT_IS_TTY_PROPERTY, {
      configurable: true,
      value,
    });
    return () => {
      if (original === undefined) {
        Reflect.deleteProperty(process.stdout, STDOUT_IS_TTY_PROPERTY);
        return;
      }
      Object.defineProperty(process.stdout, STDOUT_IS_TTY_PROPERTY, {
        configurable: true,
        value: original,
      });
    };
  }

  writeTelemetryJson(body: Record<string, unknown>): void {
    if (this.#tmpDir === undefined) throw new Error("test state directory is missing");
    writeFileSync(
      path.join(this.#tmpDir, TELEMETRY_FILE_NAME),
      JSON.stringify(body),
    );
  }

  #reset(): void {
    for (const key of ENV_KEYS) this.#saved[key] = process.env[key];
    for (const key of ENV_KEYS) delete process.env[key];
    this.captured.length = 0;
    this.identified.length = 0;
    this.aliased.length = 0;
    this.groupIdentified.length = 0;
    this.shutdownCalls = 0;
    this.flushCalls = 0;
    this.flushError = undefined;
    this.options = undefined;
    this.#tmpDir = mkdtempSync(path.join(tmpdir(), STATE_DIRECTORY_PREFIX));
    process.env.AGENTSFLEET_STATE_DIR = this.#tmpDir;
  }

  #restore(): void {
    if (this.#tmpDir !== undefined) {
      rmSync(this.#tmpDir, { recursive: true, force: true });
    }
    this.#tmpDir = undefined;
    for (const key of ENV_KEYS) {
      if (this.#saved[key] === undefined) delete process.env[key];
      else process.env[key] = this.#saved[key];
    }
  }
}

export const STUB = new AnalyticsTestHarness();

mock.module(POSTHOG_MODULE, () => ({
  PostHog: class PostHogStub {
    constructor(_key: string, options: Record<string, unknown>) {
      STUB.options = options;
    }

    capture(event: CapturedEvent): void {
      STUB.captured.push(event);
    }

    identify(payload: { distinctId: string; properties?: Record<string, unknown> }): void {
      STUB.identified.push(payload);
    }

    alias(payload: { distinctId: string; alias: string }): void {
      STUB.aliased.push(payload);
    }

    groupIdentify(payload: {
      groupType: string;
      groupKey: string;
      distinctId: string;
      properties?: Record<string, unknown>;
    }): void {
      STUB.groupIdentified.push(payload);
    }

    async shutdown(): Promise<void> {
      STUB.shutdownCalls += 1;
    }

    async _shutdown(_timeoutMs?: number): Promise<void> {
      STUB.shutdownCalls += 1;
    }

    async flush(): Promise<void> {
      STUB.flushCalls += 1;
      if (STUB.flushError !== undefined) throw STUB.flushError;
    }
  },
}));

STUB.installHooks();
