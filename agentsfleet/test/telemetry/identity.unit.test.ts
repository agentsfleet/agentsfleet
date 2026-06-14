// Identity resolution coverage. resolveIdentity persists into
// `telemetry.json` as the single source of truth (mirrors supabase's
// `~/Projects/oss/cli/apps/cli/src/shared/telemetry/identity.ts`).
// Tests redirect ZOMBIE_STATE_DIR to a per-case temp directory.

import { afterEach, beforeEach, describe, expect, it } from "bun:test";
import { Effect } from "effect";
import {
  existsSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import {
  clearDistinctId,
  resolveIdentity,
  saveDistinctId,
} from "../../src/services/telemetry/identity.ts";
import type { TelemetryConfig } from "../../src/services/telemetry/types.ts";

const UUID_PATTERN = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i;

function makeTempDir(): string {
  return mkdtempSync(path.join(tmpdir(), "agentsfleet-identity-test-"));
}

function writeTelemetry(dir: string, body: TelemetryConfig): void {
  mkdirSync(dir, { recursive: true });
  writeFileSync(path.join(dir, "telemetry.json"), JSON.stringify(body));
}

function readTelemetry(dir: string): TelemetryConfig {
  return JSON.parse(readFileSync(path.join(dir, "telemetry.json"), "utf8"));
}

const SAVED_ENV: { ZOMBIE_STATE_DIR?: string | undefined } = {};

beforeEach(() => {
  SAVED_ENV.ZOMBIE_STATE_DIR = process.env.ZOMBIE_STATE_DIR;
});

afterEach(() => {
  if (SAVED_ENV.ZOMBIE_STATE_DIR === undefined) delete process.env.ZOMBIE_STATE_DIR;
  else process.env.ZOMBIE_STATE_DIR = SAVED_ENV.ZOMBIE_STATE_DIR;
});

describe("resolveIdentity", () => {
  it("generates a fresh device_id (UUID) on first run", async () => {
    const dir = makeTempDir();
    process.env.ZOMBIE_STATE_DIR = dir;
    try {
      const id = await Effect.runPromise(resolveIdentity(dir));
      expect(id.deviceId).toMatch(UUID_PATTERN);
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  });

  it("generates a fresh session_id (UUID) on first run", async () => {
    const dir = makeTempDir();
    process.env.ZOMBIE_STATE_DIR = dir;
    try {
      const id = await Effect.runPromise(resolveIdentity(dir));
      expect(id.sessionId).toMatch(UUID_PATTERN);
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  });

  it("reports isFirstRun=true when no prior telemetry.json exists", async () => {
    const dir = makeTempDir();
    process.env.ZOMBIE_STATE_DIR = dir;
    try {
      const id = await Effect.runPromise(resolveIdentity(dir));
      expect(id.isFirstRun).toBe(true);
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  });

  it("persists telemetry.json after resolving identity", async () => {
    const dir = makeTempDir();
    process.env.ZOMBIE_STATE_DIR = dir;
    try {
      await Effect.runPromise(resolveIdentity(dir));
      expect(existsSync(path.join(dir, "telemetry.json"))).toBe(true);
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  });

  it("preserves device_id across runs", async () => {
    const dir = makeTempDir();
    process.env.ZOMBIE_STATE_DIR = dir;
    const existingDevice = "11111111-1111-4111-8111-111111111111";
    const existingSession = "22222222-2222-4222-8222-222222222222";
    writeTelemetry(dir, {
      consent: "granted",
      device_id: existingDevice,
      session_id: existingSession,
      session_last_active: Date.now(),
    });
    try {
      const id = await Effect.runPromise(resolveIdentity(dir));
      expect(id.deviceId).toBe(existingDevice);
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  });

  it("reports isFirstRun=false when telemetry.json already exists", async () => {
    const dir = makeTempDir();
    process.env.ZOMBIE_STATE_DIR = dir;
    writeTelemetry(dir, {
      consent: "granted",
      device_id: "11111111-1111-4111-8111-111111111111",
      session_id: "22222222-2222-4222-8222-222222222222",
      session_last_active: Date.now(),
    });
    try {
      const id = await Effect.runPromise(resolveIdentity(dir));
      expect(id.isFirstRun).toBe(false);
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  });

  it("preserves session_id when last activity is within the 30-minute window", async () => {
    const dir = makeTempDir();
    process.env.ZOMBIE_STATE_DIR = dir;
    const sessionId = "33333333-3333-4333-8333-333333333333";
    writeTelemetry(dir, {
      consent: "granted",
      device_id: "11111111-1111-4111-8111-111111111111",
      session_id: sessionId,
      session_last_active: Date.now() - 10 * 60 * MS_PER_SECOND,
    });
    try {
      const id = await Effect.runPromise(resolveIdentity(dir));
      expect(id.sessionId).toBe(sessionId);
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  });

  it("rotates session_id after 30 minutes of inactivity", async () => {
    const dir = makeTempDir();
    process.env.ZOMBIE_STATE_DIR = dir;
    const oldSessionId = "44444444-4444-4444-8444-444444444444";
    writeTelemetry(dir, {
      consent: "granted",
      device_id: "11111111-1111-4111-8111-111111111111",
      session_id: oldSessionId,
      session_last_active: Date.now() - 31 * 60 * MS_PER_SECOND,
    });
    try {
      const id = await Effect.runPromise(resolveIdentity(dir));
      expect(id.sessionId).not.toBe(oldSessionId);
      expect(id.sessionId).toMatch(UUID_PATTERN);
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  });

  it("persists an updated session_last_active on every call", async () => {
    const dir = makeTempDir();
    process.env.ZOMBIE_STATE_DIR = dir;
    const before = Date.now();
    writeTelemetry(dir, {
      consent: "granted",
      device_id: "11111111-1111-4111-8111-111111111111",
      session_id: "22222222-2222-4222-8222-222222222222",
      session_last_active: Date.now() - 5000,
    });
    try {
      await Effect.runPromise(resolveIdentity(dir));
      const written = readTelemetry(dir);
      expect(written.session_last_active).toBeGreaterThanOrEqual(before);
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  });

  it("surfaces persisted distinctId from telemetry.json", async () => {
    const dir = makeTempDir();
    process.env.ZOMBIE_STATE_DIR = dir;
    writeTelemetry(dir, {
      consent: "granted",
      device_id: "11111111-1111-4111-8111-111111111111",
      session_id: "22222222-2222-4222-8222-222222222222",
      session_last_active: Date.now(),
      distinct_id: "user-42",
    });
    try {
      const id = await Effect.runPromise(resolveIdentity(dir));
      expect(id.distinctId).toBe("user-42");
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  });

  it("leaves distinctId undefined when telemetry.json has no distinct_id field", async () => {
    const dir = makeTempDir();
    process.env.ZOMBIE_STATE_DIR = dir;
    try {
      const id = await Effect.runPromise(resolveIdentity(dir));
      expect(id.distinctId).toBeUndefined();
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  });
});

describe("saveDistinctId", () => {
  it("writes a new telemetry.json carrying the distinct_id", async () => {
    const dir = makeTempDir();
    process.env.ZOMBIE_STATE_DIR = dir;
    try {
      await Effect.runPromise(saveDistinctId(dir, "user-99"));
      const written = readTelemetry(dir);
      expect(written.distinct_id).toBe("user-99");
      expect(written.consent).toBe("granted"); // default when no prior config
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  });

  it("preserves the existing consent value when one is persisted", async () => {
    const dir = makeTempDir();
    process.env.ZOMBIE_STATE_DIR = dir;
    writeTelemetry(dir, {
      consent: "denied",
      device_id: "11111111-1111-4111-8111-111111111111",
      session_id: "22222222-2222-4222-8222-222222222222",
      session_last_active: 1,
    });
    try {
      await Effect.runPromise(saveDistinctId(dir, "user-100"));
      const written = readTelemetry(dir);
      expect(written.consent).toBe("denied");
      expect(written.distinct_id).toBe("user-100");
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  });
});

describe("clearDistinctId", () => {
  it("rewrites telemetry.json without the distinct_id field", async () => {
    const dir = makeTempDir();
    process.env.ZOMBIE_STATE_DIR = dir;
    writeTelemetry(dir, {
      consent: "granted",
      device_id: "11111111-1111-4111-8111-111111111111",
      session_id: "22222222-2222-4222-8222-222222222222",
      session_last_active: 1,
      distinct_id: "user-101",
    });
    try {
      await Effect.runPromise(clearDistinctId(dir));
      const written = readTelemetry(dir);
      expect(written.distinct_id).toBeUndefined();
      expect(written.consent).toBe("granted");
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  });

  it("falls back to consent=granted when no prior telemetry.json existed", async () => {
    const dir = makeTempDir();
    process.env.ZOMBIE_STATE_DIR = dir;
    try {
      await Effect.runPromise(clearDistinctId(dir));
      const written = readTelemetry(dir);
      expect(written.consent).toBe("granted");
      expect(written.distinct_id).toBeUndefined();
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  });
});
const MS_PER_SECOND = 1000 as const;
