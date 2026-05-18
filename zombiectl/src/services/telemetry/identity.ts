// Identity resolution. Mirrors
// ~/Projects/oss/cli/apps/cli/src/shared/telemetry/identity.ts but
// delegates to usezombie's pre-existing session store
// (src/lib/state.ts loadSession / saveSession), so the on-disk shape
// stays compatible with the existing telemetry runtime layer and the
// session_id timeout rotation logic isn't duplicated.
//
// resolveIdentity returns a TelemetryConfig-shaped record extended
// with `isFirstRun` (true when no prior session.json existed). The
// caller is expected to merge this into TelemetryRuntime.

import { Effect } from "effect";
import fs from "node:fs/promises";
import path from "node:path";
import {
  loadSession,
  saveSession,
  type Session,
} from "../../lib/state.ts";
import { readTelemetryConfig, writeTelemetryConfig } from "./consent.ts";
import type { TelemetryConfig } from "./types.ts";

export interface ResolvedIdentity {
  readonly deviceId: string;
  readonly sessionId: string;
  readonly distinctId: string | undefined;
  readonly isFirstRun: boolean;
}

async function sessionFileExists(configDir: string): Promise<boolean> {
  try {
    await fs.access(path.join(configDir, "session.json"));
    return true;
  } catch {
    return false;
  }
}

export const resolveIdentity = Effect.fn("telemetry.resolveIdentity")(
  function* (configDir: string) {
    const isFirstRun = !(yield* Effect.promise(() =>
      sessionFileExists(configDir),
    ));
    const session: Session = yield* Effect.promise(() => loadSession());

    // Mirror Supabase's persisted-config distinctId, but read it from
    // telemetry.json (the new dedicated file) rather than session.json
    // — session.json's shape is shared with credentials/workspaces and
    // would grow unboundedly if we stuffed analytics state there too.
    const telemetryConfig = yield* readTelemetryConfig(configDir);

    yield* Effect.promise(() =>
      saveSession({
        device_id: session.device_id,
        session_id: session.session_id,
        last_activity: Date.now(),
      }),
    );

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
