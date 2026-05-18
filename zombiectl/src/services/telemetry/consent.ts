// Consent resolution. Mirrors
// ~/Projects/oss/cli/apps/cli/src/shared/telemetry/consent.ts but reads
// process.env directly instead of going through a CliConfig service —
// usezombie's other telemetry primitives already read env directly
// (see the retired lib/analytics.ts resolveConfig).
//
// Order of precedence (first match wins):
//   1. DISABLE_TELEMETRY env (opt-out, current default behaviour)
//   2. DO_NOT_TRACK=1 env (industry-standard signal)
//   3. Persisted telemetry.json consent field
//   4. Default "granted"
//
// Telemetry is currently opt-IN: boolFromEnv(env.DISABLE_TELEMETRY,
// true) returns disabled=true when the env is unset. That semantics is
// preserved here — the new tree does not change the default; consent
// stays denied until DISABLE_TELEMETRY=0|false|off|no is set.

import { Effect } from "effect";
import fs from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import type { ConsentState, TelemetryConfig } from "./types.ts";

const FALSY_VALUES = new Set(["0", "false", "off", "no"]);

function getConfigDirSync(): string {
  return (
    process.env.ZOMBIE_STATE_DIR ||
    path.join(os.homedir(), ".config", "zombiectl")
  );
}

export const getConfigDir = Effect.sync(getConfigDirSync);

function telemetryDisabledFromEnv(env: NodeJS.ProcessEnv): boolean {
  const raw = env.DISABLE_TELEMETRY;
  if (raw == null || raw === "") return true;
  const normalized = String(raw).trim().toLowerCase();
  return !FALSY_VALUES.has(normalized);
}

function doNotTrackFromEnv(env: NodeJS.ProcessEnv): boolean {
  const raw = env.DO_NOT_TRACK;
  if (raw == null || raw === "") return false;
  return String(raw).trim() === "1";
}

export const readTelemetryConfig = (
  configDir: string,
): Effect.Effect<TelemetryConfig | null> =>
  Effect.tryPromise({
    try: () => fs.readFile(path.join(configDir, "telemetry.json"), "utf8"),
    catch: () => null,
  }).pipe(
    Effect.map((content) => JSON.parse(content) as TelemetryConfig),
    Effect.catch(() => Effect.succeed(null as TelemetryConfig | null)),
  );

export const writeTelemetryConfig = (
  config: TelemetryConfig,
  configDir: string,
): Effect.Effect<void> =>
  Effect.tryPromise({
    try: async () => {
      await fs.mkdir(configDir, { recursive: true, mode: 0o700 });
      await fs.writeFile(
        path.join(configDir, "telemetry.json"),
        JSON.stringify(config, null, 2),
        { mode: 0o600 },
      );
    },
    catch: () => undefined,
  }).pipe(Effect.ignore);

export const getEffectiveConsent = (
  config: TelemetryConfig | null,
  env: NodeJS.ProcessEnv = process.env,
): ConsentState => {
  if (telemetryDisabledFromEnv(env)) return "denied";
  if (doNotTrackFromEnv(env)) return "denied";
  return config?.consent ?? "granted";
};
