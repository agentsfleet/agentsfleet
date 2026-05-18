// Consent resolution. Reads process.env directly (no CliConfig
// service field in usezombie yet — see M75 follow-up).
//
// Order of precedence (first match wins), mirrors supabase
// getEffectiveConsent in apps/cli/src/shared/telemetry/consent.ts:
//   1. ZOMBIE_TELEMETRY_DISABLED=1 env (opt-out kill switch)
//   2. DO_NOT_TRACK=1 env (industry-standard signal)
//   3. Persisted telemetry.json consent field
//   4. Default "granted"
//
// Telemetry is opt-OUT (i.e. enabled by default). Only the literal
// string "1" opts out; anything else (including unset) keeps
// telemetry on. Matches supabase SUPABASE_TELEMETRY_DISABLED behavior.

import { Effect } from "effect";
import fs from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import type { ConsentState, TelemetryConfig } from "./types.ts";

function getConfigDirSync(): string {
  return (
    process.env.ZOMBIE_STATE_DIR ||
    path.join(os.homedir(), ".config", "zombiectl")
  );
}

export const getConfigDir = Effect.sync(getConfigDirSync);

function telemetryDisabledFromEnv(env: NodeJS.ProcessEnv): boolean {
  const raw = env.ZOMBIE_TELEMETRY_DISABLED;
  return raw != null && String(raw).trim() === "1";
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
