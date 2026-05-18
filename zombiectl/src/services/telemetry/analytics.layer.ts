// analyticsLayer — PostHog product-analytics implementation. Mirrors
// ~/Projects/oss/cli/apps/cli/src/shared/telemetry/analytics.layer.ts.
//
// Owns: PostHog client construction (env: ZOMBIE_POSTHOG_KEY,
// ZOMBIE_POSTHOG_HOST), consent gating (no-op when denied), base
// property merging, CurrentAnalyticsContext merging on every capture,
// and shutdown via Effect.addFinalizer (Scoped layer).
//
// The previous PostHog construction lived in src/lib/analytics.ts —
// it is retired in the cutover commit; this layer is the single
// owner of `posthog-node` in the new tree.
//
// Telemetry failures are swallowed inside capture — Analytics never
// blocks user-facing UX. The dispatcher does NOT wrap captures in
// Effect.ignore (the wrapper is here).

import { PostHog } from "posthog-node";
import { Effect, Layer } from "effect";
import { CurrentAnalyticsContext, type AnalyticsContext } from "./analytics-context.ts";
import { Analytics } from "./analytics.service.ts";
import { TelemetryRuntime } from "./runtime.service.ts";
import { telemetryRuntimeLayer } from "./runtime.layer.ts";

const DEFAULT_POSTHOG_HOST = "https://us.i.posthog.com";
const DEFAULT_POSTHOG_KEY = [
  "phc_XmuRIXBST",
  "Rfxka7IgfkU0V",
  "PMD3LDRR3IqIL",
  "XNg3bXzv",
].join("");

function resolvePosthogKey(env: NodeJS.ProcessEnv): string {
  return env.ZOMBIE_POSTHOG_KEY || DEFAULT_POSTHOG_KEY;
}

function resolvePosthogHost(env: NodeJS.ProcessEnv): string {
  return env.ZOMBIE_POSTHOG_HOST || DEFAULT_POSTHOG_HOST;
}

function stripUndefined(
  properties: Record<string, unknown>,
): Record<string, unknown> {
  return Object.fromEntries(
    Object.entries(properties).filter(([, value]) => value !== undefined),
  );
}

function contextProperties(context: AnalyticsContext): Record<string, unknown> {
  return stripUndefined({
    command_run_id: context.command_run_id,
    command: context.command,
    flags_used: context.flags_used,
    flag_values: context.flag_values,
  });
}

function resolveGroups(
  context: AnalyticsContext,
): { workspace: string } | undefined {
  if (context.groups?.workspace !== undefined) {
    return { workspace: context.groups.workspace };
  }
  return undefined;
}

const noopAnalytics = Analytics.of({
  capture: () => Effect.void,
  identify: () => Effect.void,
  alias: () => Effect.void,
  groupIdentify: () => Effect.void,
});

export const analyticsLayer: Layer.Layer<Analytics, never, TelemetryRuntime> =
  Layer.effect(
    Analytics,
    Effect.gen(function* () {
      const runtime = yield* TelemetryRuntime;

      if (runtime.consent !== "granted") {
        return noopAnalytics;
      }

      const posthogKey = resolvePosthogKey(process.env);
      if (posthogKey.length === 0) {
        return noopAnalytics;
      }

      const client = new PostHog(posthogKey, {
        host: resolvePosthogHost(process.env),
        flushAt: 1,
        flushInterval: 0,
      });
      yield* Effect.addFinalizer(() =>
        Effect.promise(() => client.shutdown()).pipe(Effect.ignore),
      );

      const baseProperties = stripUndefined({
        platform: "cli",
        schema_version: 1,
        device_id: runtime.deviceId,
        $session_id: runtime.sessionId,
        is_first_run: runtime.isFirstRun,
        is_tty: runtime.isTty,
        is_ci: runtime.isCi,
        os: runtime.os,
        arch: runtime.arch,
        cli_version: runtime.cliVersion,
      });

      const capture = (event: string, properties: Record<string, unknown> = {}) =>
        Effect.gen(function* () {
          const context = yield* CurrentAnalyticsContext;
          const groups = resolveGroups(context);
          try {
            client.capture({
              event,
              distinctId: context.distinct_id ?? runtime.distinctId ?? runtime.deviceId,
              ...(groups === undefined ? {} : { groups }),
              properties: {
                ...baseProperties,
                ...contextProperties(context),
                ...stripUndefined(properties),
              },
            });
          } catch {
            // never block CLI UX on a telemetry fault
          }
        });

      const identify = (
        distinctId: string,
        properties: Record<string, unknown> = {},
      ) =>
        Effect.sync(() => {
          try {
            client.identify({
              distinctId,
              properties: stripUndefined({
                cli_version: runtime.cliVersion,
                os: runtime.os,
                arch: runtime.arch,
                ...properties,
              }),
            });
          } catch {
            // ignore
          }
        });

      const alias = (distinctId: string, aliasValue: string) =>
        Effect.sync(() => {
          try {
            client.alias({ distinctId, alias: aliasValue });
          } catch {
            // ignore
          }
        });

      const groupIdentify = (
        groupType: string,
        groupKey: string,
        properties: Record<string, unknown> = {},
      ) =>
        Effect.gen(function* () {
          const context = yield* CurrentAnalyticsContext;
          try {
            client.groupIdentify({
              groupType,
              groupKey,
              distinctId: context.distinct_id ?? runtime.distinctId ?? runtime.deviceId,
              properties: stripUndefined(properties),
            });
          } catch {
            // ignore
          }
        });

      return Analytics.of({
        capture,
        identify,
        alias,
        groupIdentify,
      });
    }),
  ).pipe(Layer.provide(telemetryRuntimeLayer));

export const analyticsInternals = {
  DEFAULT_POSTHOG_HOST,
  DEFAULT_POSTHOG_KEY,
  resolvePosthogKey,
  resolvePosthogHost,
  stripUndefined,
  contextProperties,
  resolveGroups,
} as const;
