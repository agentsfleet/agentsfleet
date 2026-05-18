// Identity resolution. Mirrors
// ~/Projects/oss/cli/apps/cli/src/shared/telemetry/identity.ts but
// delegates deviceId/sessionId rotation to usezombie's pre-existing
// session store (src/lib/state.ts loadSession / saveSession). The
// telemetry.json file (consent + distinct_id) is the dedicated
// telemetry persistence; its absence signals first-run and triggers
// a bootstrap write with consent: "granted" (default-ON model).
//
// resolveIdentity returns a TelemetryConfig-shaped record extended
// with `isFirstRun` (true when telemetry.json was absent on entry).
// The caller is expected to merge this into TelemetryRuntime.

import { Effect } from "effect";
import {
  loadSession,
  saveSession,
  type Session,
} from "../../lib/state.ts";
import { readTelemetryConfig, writeTelemetryConfig } from "./consent.ts";
import type { TelemetryConfig } from "./types.ts";

interface ResolvedIdentity {
  readonly deviceId: string;
  readonly sessionId: string;
  readonly distinctId: string | undefined;
  readonly isFirstRun: boolean;
}

export const resolveIdentity = Effect.fn("telemetry.resolveIdentity")(
  function* (configDir: string) {
    // First-run signal is the absence of telemetry.json (mirrors
    // supabase identity.ts:11-24). When absent, bootstrap the file with
    // consent: "granted" so the default-ON model is durable across
    // upgrades. session.json continues to back deviceId/sessionId since
    // that store predates telemetry.json in usezombie.
    const telemetryConfig = yield* readTelemetryConfig(configDir);
    const isFirstRun = telemetryConfig === null;
    const session: Session = yield* Effect.promise(() => loadSession());

    yield* Effect.promise(() =>
      saveSession({
        device_id: session.device_id,
        session_id: session.session_id,
        last_activity: Date.now(),
      }),
    );

    if (isFirstRun) {
      const bootstrap: TelemetryConfig = {
        consent: "granted",
        device_id: session.device_id,
        session_id: session.session_id,
        session_last_active: Date.now(),
      };
      yield* writeTelemetryConfig(bootstrap, configDir);
    }

    return {
      deviceId: session.device_id,
      sessionId: session.session_id,
      distinctId: telemetryConfig?.distinct_id,
      isFirstRun,
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
