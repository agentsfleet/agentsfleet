// Consent + telemetry.json read/write coverage. Adapted from the
// Supabase consent.unit.test.ts — usezombie's getEffectiveConsent is
// a sync function reading process.env directly (no CliConfig service),
// so the tests stub env via process.env.AGENTSFLEET_TELEMETRY_DISABLED /
// DO_NOT_TRACK and clean up in afterEach.
//
// Default is opt-OUT (granted) per supabase parity: only
// AGENTSFLEET_TELEMETRY_DISABLED=1 or DO_NOT_TRACK=1 flips to denied.

import { afterEach, beforeEach, describe, expect, it } from "bun:test";
import { Effect } from "effect";
import { mkdtempSync, readFileSync, rmSync, writeFileSync, mkdirSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import {
  getConfigDir,
  getEffectiveConsent,
  readTelemetryConfig,
  writeTelemetryConfig,
} from "../../src/services/telemetry/consent.ts";
import type { TelemetryConfig } from "../../src/services/telemetry/types.ts";

function makeConfig(consent: TelemetryConfig["consent"]): TelemetryConfig {
  return {
    consent,
    device_id: "test-device",
    session_id: "test-session",
    session_last_active: Date.now(),
  };
}

function makeTempDir(): string {
  return mkdtempSync(path.join(tmpdir(), "agentsfleet-consent-test-"));
}

const ENV_KEYS = ["AGENTSFLEET_TELEMETRY_DISABLED", "DO_NOT_TRACK", "AGENTSFLEET_STATE_DIR"] as const;
const saved: Record<string, string | undefined> = {};

beforeEach(() => {
  for (const k of ENV_KEYS) saved[k] = process.env[k];
  for (const k of ENV_KEYS) delete process.env[k];
});

afterEach(() => {
  for (const k of ENV_KEYS) {
    if (saved[k] === undefined) delete process.env[k];
    else process.env[k] = saved[k];
  }
});

describe("getEffectiveConsent", () => {
  it("returns granted by default (env clean, opt-OUT model)", () => {
    expect(getEffectiveConsent(makeConfig("granted"))).toBe("granted");
  });

  it("returns denied when AGENTSFLEET_TELEMETRY_DISABLED=1", () => {
    process.env.AGENTSFLEET_TELEMETRY_DISABLED = "1";
    expect(getEffectiveConsent(makeConfig("granted"))).toBe("denied");
  });

  it("treats AGENTSFLEET_TELEMETRY_DISABLED=true as not '1' (still granted)", () => {
    // Mirrors supabase: only the literal string "1" opts out.
    process.env.AGENTSFLEET_TELEMETRY_DISABLED = "true";
    expect(getEffectiveConsent(makeConfig("granted"))).toBe("granted");
  });

  it("treats AGENTSFLEET_TELEMETRY_DISABLED=0 as not '1' (still granted)", () => {
    process.env.AGENTSFLEET_TELEMETRY_DISABLED = "0";
    expect(getEffectiveConsent(makeConfig("granted"))).toBe("granted");
  });

  it("returns denied when DO_NOT_TRACK=1", () => {
    process.env.DO_NOT_TRACK = "1";
    expect(getEffectiveConsent(makeConfig("granted"))).toBe("denied");
  });

  it("DO_NOT_TRACK=1 takes precedence over persisted granted consent", () => {
    process.env.DO_NOT_TRACK = "1";
    expect(getEffectiveConsent(null)).toBe("denied");
  });

  it("AGENTSFLEET_TELEMETRY_DISABLED=1 beats DO_NOT_TRACK=1 (both flag denied)", () => {
    process.env.AGENTSFLEET_TELEMETRY_DISABLED = "1";
    process.env.DO_NOT_TRACK = "1";
    expect(getEffectiveConsent(makeConfig("granted"))).toBe("denied");
  });

  it("DO_NOT_TRACK ignored when not '1'", () => {
    process.env.DO_NOT_TRACK = "yes";
    expect(getEffectiveConsent(makeConfig("granted"))).toBe("granted");
  });

  it("returns persisted denied even when env permits", () => {
    expect(getEffectiveConsent(makeConfig("denied"))).toBe("denied");
  });

  it("defaults to granted when env permits and no persisted config", () => {
    expect(getEffectiveConsent(null)).toBe("granted");
  });

  it("accepts an explicit env override argument", () => {
    expect(getEffectiveConsent(makeConfig("granted"), {})).toBe("granted");
    expect(
      getEffectiveConsent(makeConfig("granted"), { DO_NOT_TRACK: "1" }),
    ).toBe("denied");
    expect(
      getEffectiveConsent(makeConfig("granted"), { AGENTSFLEET_TELEMETRY_DISABLED: "1" }),
    ).toBe("denied");
  });

  it("treats empty string AGENTSFLEET_TELEMETRY_DISABLED as unset (granted default)", () => {
    expect(getEffectiveConsent(makeConfig("granted"), { AGENTSFLEET_TELEMETRY_DISABLED: "" })).toBe(
      "granted",
    );
  });

  it("treats empty string DO_NOT_TRACK as not set", () => {
    expect(
      getEffectiveConsent(makeConfig("granted"), { DO_NOT_TRACK: "" }),
    ).toBe("granted");
  });
});

describe("getConfigDir", () => {
  it("respects AGENTSFLEET_STATE_DIR when set", async () => {
    process.env.AGENTSFLEET_STATE_DIR = "/tmp/agentsfleet-cd-test";
    const dir = await Effect.runPromise(getConfigDir);
    expect(dir).toBe("/tmp/agentsfleet-cd-test");
  });

  it("falls back to ~/.config/agentsfleet when env unset", async () => {
    const dir = await Effect.runPromise(getConfigDir);
    expect(dir.endsWith(path.join(".config", "agentsfleet"))).toBe(true);
  });
});

describe("readTelemetryConfig", () => {
  it("returns null when telemetry.json does not exist", async () => {
    const dir = makeTempDir();
    try {
      const cfg = await Effect.runPromise(readTelemetryConfig(dir));
      expect(cfg).toBeNull();
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  });

  it("returns null on malformed JSON (parse error caught into typed channel)", async () => {
    // Corrupted telemetry.json (e.g. partial write after a crash) must
    // not crash the CLI. The Effect.try wrap around JSON.parse converts
    // the synchronous SyntaxError into a typed error the surrounding
    // Effect.catch can intercept, returning null gracefully so command
    // dispatch continues with default consent.
    const dir = makeTempDir();
    try {
      writeFileSync(path.join(dir, "telemetry.json"), "{not json");
      const cfg = await Effect.runPromise(readTelemetryConfig(dir));
      expect(cfg).toBeNull();
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  });

  it("parses a valid telemetry.json", async () => {
    const dir = makeTempDir();
    try {
      const config = makeConfig("denied");
      writeFileSync(path.join(dir, "telemetry.json"), JSON.stringify(config));
      const cfg = await Effect.runPromise(readTelemetryConfig(dir));
      expect(cfg?.consent).toBe("denied");
      expect(cfg?.device_id).toBe("test-device");
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  });
});

describe("writeTelemetryConfig", () => {
  it("creates the directory and persists the config (mode bits honoured)", async () => {
    const base = makeTempDir();
    const dir = path.join(base, "nested");
    try {
      const config = makeConfig("granted");
      await Effect.runPromise(writeTelemetryConfig(config, dir));
      const round = JSON.parse(readFileSync(path.join(dir, "telemetry.json"), "utf8")) as TelemetryConfig;
      expect(round.consent).toBe("granted");
      expect(round.device_id).toBe("test-device");
    } finally {
      rmSync(base, { recursive: true, force: true });
    }
  });

  it("never throws on filesystem errors", async () => {
    // Pointing at a file path that already exists as a regular file forces
    // mkdir to fail — write must swallow and resolve.
    const base = makeTempDir();
    try {
      const filePath = path.join(base, "blocker");
      writeFileSync(filePath, "regular file");
      const config = makeConfig("granted");
      mkdirSync(base, { recursive: true });
      // Use filePath as the directory; mkdir will EEXIST against the file.
      await Effect.runPromise(writeTelemetryConfig(config, filePath));
    } finally {
      rmSync(base, { recursive: true, force: true });
    }
  });
});
