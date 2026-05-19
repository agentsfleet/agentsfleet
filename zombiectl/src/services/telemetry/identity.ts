// Identity resolution. Mirrors
// ~/Projects/oss/cli/apps/cli/src/shared/telemetry/identity.ts —
// `telemetry.json` is the single source of truth for `device_id`,
// `session_id`, `session_last_active`, `distinct_id`, and `consent`.
// `session_id` rotates after `SESSION_TIMEOUT_MS` of inactivity;
// `device_id` is permanent for the install lifetime.
//
// resolveIdentity returns a TelemetryConfig-shaped record extended
// with `isFirstRun` (true when `telemetry.json` was absent on entry).
// The caller is expected to merge this into TelemetryRuntime.

import { Effect } from "effect";
import { randomUUID } from "node:crypto";
import { readTelemetryConfig, writeTelemetryConfig } from "./consent.ts";
import type { TelemetryConfig } from "./types.ts";

// Pinned from Supabase's identity.ts. Inactivity past this rotates
// `session_id`; `device_id` stays permanent.
const SESSION_TIMEOUT_MS = 30 * 60 * 1000;

interface ResolvedIdentity {
  readonly deviceId: string;
  readonly sessionId: string;
  readonly distinctId: string | undefined;
  readonly isFirstRun: boolean;
}

export const resolveIdentity = Effect.fn("telemetry.resolveIdentity")(
  function* (configDir: string) {
    const config = yield* readTelemetryConfig(configDir);
    const now = Date.now();

    if (!config) {
      // First-run: mint fresh identity + bootstrap with default-ON
      // consent. Mirrors supabase identity.ts:11-24.
      const bootstrap: TelemetryConfig = {
        consent: "granted",
        device_id: randomUUID(),
        session_id: randomUUID(),
        session_last_active: now,
      };
      yield* writeTelemetryConfig(bootstrap, configDir);
      return {
        deviceId: bootstrap.device_id,
        sessionId: bootstrap.session_id,
        distinctId: undefined,
        isFirstRun: true,
      } satisfies ResolvedIdentity;
    }

    const isSessionExpired = now - config.session_last_active > SESSION_TIMEOUT_MS;
    const sessionId = isSessionExpired ? randomUUID() : config.session_id;

    yield* writeTelemetryConfig(
      { ...config, session_id: sessionId, session_last_active: now },
      configDir,
    );
    return {
      deviceId: config.device_id,
      sessionId,
      distinctId: config.distinct_id,
      isFirstRun: false,
    } satisfies ResolvedIdentity;
  },
);

export const saveDistinctId = Effect.fn("telemetry.saveDistinctId")(
  function* (configDir: string, distinctId: string) {
    const identity = yield* resolveIdentity(configDir);
    const existing = yield* readTelemetryConfig(configDir);
    const nextConfig: TelemetryConfig = {
      consent: existing?.consent ?? "granted",
      device_id: identity.deviceId,
      session_id: identity.sessionId,
      session_last_active: Date.now(),
      distinct_id: distinctId,
    };
    yield* writeTelemetryConfig(nextConfig, configDir);
  },
);

export const clearDistinctId = Effect.fn("telemetry.clearDistinctId")(
  function* (configDir: string) {
    const identity = yield* resolveIdentity(configDir);
    const existing = yield* readTelemetryConfig(configDir);
    const nextConfig: TelemetryConfig = {
      consent: existing?.consent ?? "granted",
      device_id: identity.deviceId,
      session_id: identity.sessionId,
      session_last_active: Date.now(),
    };
    yield* writeTelemetryConfig(nextConfig, configDir);
  },
);
