// Consent resolution. Reads process.env directly (no CliConfig
// service field in usezombie yet — see M75 follow-up).
//
// Order of precedence (first match wins), mirrors supabase
// getEffectiveConsent in apps/cli/src/shared/telemetry/consent.ts:
//   1. AGENTSFLEET_TELEMETRY_DISABLED=1 env (opt-out kill switch)
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
    process.env.AGENTSFLEET_STATE_DIR ||
    path.join(os.homedir(), ".config", "agentsfleet")
  );
}

export const getConfigDir = Effect.sync(getConfigDirSync);

function telemetryDisabledFromEnv(env: NodeJS.ProcessEnv): boolean {
  const raw = env.AGENTSFLEET_TELEMETRY_DISABLED;
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
    try: () => fs.readFile(path.join(configDir, TELEMETRY_FILE_NAME), "utf8"),
    catch: () => null,
  }).pipe(
    // JSON.parse throws synchronously on malformed input; wrap in
    // Effect.try so the SyntaxError lands in the typed error channel
    // instead of escaping as an Effect defect that Effect.catch can't
    // intercept. Mirrors the ai-tool.layer.ts pattern landed earlier
    // in this PR.
    Effect.flatMap((content) =>
      Effect.try({
        try: () => JSON.parse(content) as TelemetryConfig,
        catch: () => null,
      }),
    ),
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
        path.join(configDir, TELEMETRY_FILE_NAME),
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
  if (telemetryDisabledFromEnv(env)) return CONSENT_DENIED;
  if (doNotTrackFromEnv(env)) return CONSENT_DENIED;
  return config?.consent ?? "granted";
};
const CONSENT_DENIED = "denied" as const;
const TELEMETRY_FILE_NAME = "telemetry.json" as const;
