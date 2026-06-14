// telemetryRuntimeLayer — process-start telemetry bootstrap. Mirrors
// ~/Projects/oss/cli/apps/cli/src/shared/telemetry/runtime.layer.ts.
//
// Resolves once at the start of the CLI invocation, against:
//   - persisted telemetry.json (consent state, distinct_id)
//   - persisted session.json (device_id, session_id with timeout
//     rotation — delegated to identity.ts → lib/state.ts)
//   - process.env (ZOMBIE_TELEMETRY_DISABLED, DO_NOT_TRACK, CI vars, ZOMBIE_TELEMETRY_DEBUG)
//   - node:os (platform, arch, hostname)
//   - process.stdout.isTTY for TTY detection
//
// On first run with consent granted and a TTY, prints a one-time
// notice (matching Supabase's @clack/prompts note). usezombie writes
// to stderr directly via process.stderr.write — the Output service
// isn't available yet at this point (it's a co-dependent layer).

import { readFileSync } from "node:fs";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { Effect, Layer } from "effect";
import {
  getConfigDir,
  getEffectiveConsent,
  readTelemetryConfig,
} from "./consent.ts";
import { resolveIdentity } from "./identity.ts";
import { TelemetryRuntime } from "./runtime.service.ts";

const PARENT_DIR_SEGMENT = ".." as const;

// Read package.json once at module load — same shape as cli.ts VERSION
// resolution. cli.ts re-imports this for its own export; avoids the
// import cycle that "telemetry/runtime.layer.ts → cli.ts" would create.
const PKG_JSON_PATH = path.join(
  path.dirname(fileURLToPath(import.meta.url)),
  PARENT_DIR_SEGMENT,
  PARENT_DIR_SEGMENT,
  PARENT_DIR_SEGMENT,
  "package.json",
);
const pkgJson = JSON.parse(readFileSync(PKG_JSON_PATH, "utf8")) as { version: string };
const CLI_VERSION: string = pkgJson.version;

const CI_ENV_VARS = [
  "CI",
  "GITHUB_ACTIONS",
  "GITLAB_CI",
  "CIRCLECI",
  "JENKINS_URL",
  "BUILDKITE",
];

const FIRST_RUN_NOTICE = [
  "",
  "  usezombie collects anonymous usage data to improve the CLI.",
  "  Opt out by setting ZOMBIE_TELEMETRY_DISABLED=1 (or DO_NOT_TRACK=1).",
  "  Learn more: https://docs.agentsfleet.net/cli/telemetry",
  "",
  "",
].join("\n");

function detectIsCi(env: NodeJS.ProcessEnv): boolean {
  for (const key of CI_ENV_VARS) {
    if (env[key] !== undefined && env[key] !== "") return true;
  }
  return false;
}

function detectShowDebug(env: NodeJS.ProcessEnv): boolean {
  const raw = env.ZOMBIE_TELEMETRY_DEBUG;
  return raw === "1" || raw === "true";
}

function ephemeralIdentity(): {
  deviceId: string;
  sessionId: string;
  distinctId: string | undefined;
  isFirstRun: boolean;
} {
  return {
    deviceId: crypto.randomUUID(),
    sessionId: crypto.randomUUID(),
    distinctId: undefined,
    isFirstRun: false,
  };
}

export const telemetryRuntimeLayer = Layer.effect(
  TelemetryRuntime,
  Effect.gen(function* () {
    const configDir = yield* getConfigDir;
    const tracesDir = `${configDir}/traces`;
    const config = yield* readTelemetryConfig(configDir);
    const consent = getEffectiveConsent(config);
    const isTty = process.stdout.isTTY === true;
    const isCi = detectIsCi(process.env);

    let identity: {
      deviceId: string;
      sessionId: string;
      distinctId: string | undefined;
      isFirstRun: boolean;
    };

    if (consent === "granted") {
      identity = yield* resolveIdentity(configDir);
      if (identity.isFirstRun && isTty) {
        process.stderr.write(FIRST_RUN_NOTICE);
      }
    } else {
      identity = ephemeralIdentity();
    }

    const base = {
      configDir,
      tracesDir,
      consent,
      showDebug: detectShowDebug(process.env),
      deviceId: identity.deviceId,
      sessionId: identity.sessionId,
      isFirstRun: identity.isFirstRun,
      isTty,
      isCi,
      os: os.platform(),
      arch: os.arch(),
      cliVersion: CLI_VERSION,
    };
    return TelemetryRuntime.of(
      identity.distinctId !== undefined
        ? { ...base, distinctId: identity.distinctId }
        : base,
    );
  }),

);
