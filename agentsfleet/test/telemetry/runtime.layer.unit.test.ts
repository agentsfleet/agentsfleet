// telemetryRuntimeLayer coverage. usezombie's runtime layer reads
// process.env / process.stdout.isTTY / os directly — there is no
// CliConfig service to mock. Tests stub env (AGENTSFLEET_TELEMETRY_DISABLED,
// DO_NOT_TRACK, AGENTSFLEET_TELEMETRY_DEBUG, CI, AGENTSFLEET_STATE_DIR) plus
// process.stdout.isTTY for each case and clean up in afterEach.

import { afterEach, beforeEach, describe, expect, it } from "bun:test";
import { Effect } from "effect";
import { existsSync, mkdtempSync, rmSync, writeFileSync, mkdirSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import { telemetryRuntimeLayer } from "../../src/services/telemetry/runtime.layer.ts";
import { TelemetryRuntime } from "../../src/services/telemetry/runtime.service.ts";

const ENV_KEYS = [
  "AGENTSFLEET_TELEMETRY_DISABLED",
  "DO_NOT_TRACK",
  "AGENTSFLEET_TELEMETRY_DEBUG",
  "AGENTSFLEET_STATE_DIR",
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
});

afterEach(() => {
  for (const k of ENV_KEYS) {
    if (saved[k] === undefined) delete process.env[k];
    else process.env[k] = saved[k];
  }
  Object.defineProperty(process.stdout, "isTTY", { value: savedIsTty, configurable: true });
});

function makeTempDir(): string {
  return mkdtempSync(path.join(tmpdir(), "agentsfleet-runtime-test-"));
}

function setTty(value: boolean): void {
  Object.defineProperty(process.stdout, "isTTY", { value, configurable: true });
}

async function buildRuntime() {
  return await Effect.runPromise(
    Effect.gen(function* () {
      return yield* TelemetryRuntime;
    }).pipe(Effect.provide(telemetryRuntimeLayer)),
  );
}

describe("telemetryRuntimeLayer", () => {
  it("grants + bootstraps telemetry.json on first run (opt-OUT default, supabase parity)", async () => {
    const dir = makeTempDir();
    process.env.AGENTSFLEET_STATE_DIR = dir;
    setTty(false);
    try {
      const rt = await buildRuntime();
      expect(rt.consent).toBe("granted");
      expect(rt.isFirstRun).toBe(true);
      expect(existsSync(path.join(dir, "telemetry.json"))).toBe(true);
      expect(typeof rt.deviceId).toBe("string");
      expect(typeof rt.sessionId).toBe("string");
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  });

  it("denies when AGENTSFLEET_TELEMETRY_DISABLED=1 (env clean otherwise)", async () => {
    const dir = makeTempDir();
    process.env.AGENTSFLEET_STATE_DIR = dir;
    process.env.AGENTSFLEET_TELEMETRY_DISABLED = "1";
    setTty(false);
    try {
      const rt = await buildRuntime();
      expect(rt.consent).toBe("denied");
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  });

  it("denies when DO_NOT_TRACK=1 (env wins over telemetry.json granted)", async () => {
    const dir = makeTempDir();
    process.env.AGENTSFLEET_STATE_DIR = dir;
    process.env.DO_NOT_TRACK = "1";
    mkdirSync(dir, { recursive: true });
    writeFileSync(
      path.join(dir, "telemetry.json"),
      JSON.stringify({
        consent: "granted",
        device_id: "x",
        session_id: "y",
        session_last_active: Date.now(),
      }),
    );
    setTty(false);
    try {
      const rt = await buildRuntime();
      expect(rt.consent).toBe("denied");
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  });

  it("marks the first granted invocation as isFirstRun=true", async () => {
    const dir = makeTempDir();
    process.env.AGENTSFLEET_STATE_DIR = dir;
    setTty(false);
    try {
      const rt = await buildRuntime();
      expect(rt.consent).toBe("granted");
      expect(rt.isFirstRun).toBe(true);
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  });

  it("emits the first-run notice on stderr when granted + TTY", async () => {
    const dir = makeTempDir();
    process.env.AGENTSFLEET_STATE_DIR = dir;
    setTty(true);
    const chunks: string[] = [];
    const original = process.stderr.write.bind(process.stderr);
    process.stderr.write = ((chunk: unknown) => {
      chunks.push(String(chunk));
      return true;
    }) as typeof process.stderr.write;
    try {
      const rt = await buildRuntime();
      expect(rt.isTty).toBe(true);
      expect(chunks.join("")).toContain("usezombie");
    } finally {
      process.stderr.write = original;
      rmSync(dir, { recursive: true, force: true });
    }
  });

  it("populates os / arch / cliVersion / tracesDir / configDir", async () => {
    const dir = makeTempDir();
    process.env.AGENTSFLEET_STATE_DIR = dir;
    setTty(false);
    try {
      const rt = await buildRuntime();
      expect(rt.configDir).toBe(dir);
      expect(rt.tracesDir).toBe(`${dir}/traces`);
      expect(typeof rt.os).toBe("string");
      expect(typeof rt.arch).toBe("string");
      expect(typeof rt.cliVersion).toBe("string");
      expect(rt.cliVersion.length).toBeGreaterThan(0);
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  });

  it("detects CI via the CI env var", async () => {
    const dir = makeTempDir();
    process.env.AGENTSFLEET_STATE_DIR = dir;
    process.env.CI = "true";
    setTty(false);
    try {
      const rt = await buildRuntime();
      expect(rt.isCi).toBe(true);
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  });

  it("detects CI via GITHUB_ACTIONS env var", async () => {
    const dir = makeTempDir();
    process.env.AGENTSFLEET_STATE_DIR = dir;
    process.env.GITHUB_ACTIONS = "1";
    setTty(false);
    try {
      const rt = await buildRuntime();
      expect(rt.isCi).toBe(true);
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  });

  it("isCi=false when no CI variable is set", async () => {
    const dir = makeTempDir();
    process.env.AGENTSFLEET_STATE_DIR = dir;
    setTty(false);
    try {
      const rt = await buildRuntime();
      expect(rt.isCi).toBe(false);
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  });

  it("respects AGENTSFLEET_TELEMETRY_DEBUG=1 (showDebug=true)", async () => {
    const dir = makeTempDir();
    process.env.AGENTSFLEET_STATE_DIR = dir;
    process.env.AGENTSFLEET_TELEMETRY_DEBUG = "1";
    setTty(false);
    try {
      const rt = await buildRuntime();
      expect(rt.showDebug).toBe(true);
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  });

  it("respects AGENTSFLEET_TELEMETRY_DEBUG=true (showDebug=true)", async () => {
    const dir = makeTempDir();
    process.env.AGENTSFLEET_STATE_DIR = dir;
    process.env.AGENTSFLEET_TELEMETRY_DEBUG = "true";
    setTty(false);
    try {
      const rt = await buildRuntime();
      expect(rt.showDebug).toBe(true);
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  });

  it("showDebug=false when AGENTSFLEET_TELEMETRY_DEBUG is not '1' or 'true'", async () => {
    const dir = makeTempDir();
    process.env.AGENTSFLEET_STATE_DIR = dir;
    process.env.AGENTSFLEET_TELEMETRY_DEBUG = "yes";
    setTty(false);
    try {
      const rt = await buildRuntime();
      expect(rt.showDebug).toBe(false);
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  });

  it("surfaces persisted distinctId when telemetry.json carries one", async () => {
    const dir = makeTempDir();
    process.env.AGENTSFLEET_STATE_DIR = dir;
    mkdirSync(dir, { recursive: true });
    writeFileSync(
      path.join(dir, "telemetry.json"),
      JSON.stringify({
        consent: "granted",
        device_id: "anything",
        session_id: "anything",
        session_last_active: Date.now(),
        distinct_id: "user-123",
      }),
    );
    setTty(false);
    try {
      const rt = await buildRuntime();
      expect(rt.distinctId).toBe("user-123");
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  });
});
