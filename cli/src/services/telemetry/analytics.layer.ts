// analyticsLayer — PostHog product-analytics implementation.
//
// Owns: PostHog client construction (key/host read from CliConfig,
// itself resolved from env AGENTSFLEET_TELEMETRY_POSTHOG_KEY /
// AGENTSFLEET_TELEMETRY_POSTHOG_HOST at layer construction), consent
// gating (no-op when denied), base property merging,
// CurrentAnalyticsContext merging on every capture, and shutdown via
// Effect.addFinalizer (Scoped layer). The single owner of
// `posthog-node` in the codebase.
//
// Telemetry failures bubble through the Effect cause channel; call
// sites swallow with .pipe(Effect.ignore). Matches supabase
// apps/cli/src/shared/telemetry/analytics.layer.ts — no try/catch
// inside the emit functions.

import { PostHog } from "posthog-node";
import { Effect, Layer, Option } from "effect";
import { CliConfig } from "../config.ts";
import { CurrentAnalyticsContext, type AnalyticsContext } from "./analytics-context.ts";
import { Analytics } from "./analytics.service.ts";
import { AiTool } from "./ai-tool.service.ts";
import { aiToolLayer } from "./ai-tool.layer.ts";
import { TelemetryRuntime, type TelemetryRuntime as TelemetryRuntimeValue } from "./runtime.service.ts";
import { telemetryRuntimeLayer } from "./runtime.layer.ts";

const AI_TOOL_CI = "ci";
const AI_TOOL_UNKNOWN_NON_INTERACTIVE = "unknown_non_interactive";

type AiToolRuntime = Pick<TelemetryRuntimeValue, "isCi" | "isTty">;

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

function resolveAiToolName(
  name: Option.Option<string>,
  runtime: AiToolRuntime,
): string | undefined {
  if (Option.isSome(name)) return name.value;
  if (runtime.isCi) return AI_TOOL_CI;
  if (runtime.isTty) return undefined;
  return AI_TOOL_UNKNOWN_NON_INTERACTIVE;
}

const noopAnalytics = Analytics.of({
  capture: () => Effect.void,
  identify: () => Effect.void,
  alias: () => Effect.void,
  groupIdentify: () => Effect.void,
});

export const analyticsLayer = Layer.effect(
    Analytics,
    Effect.gen(function* () {
      const runtime = yield* TelemetryRuntime;
      const aiTool = yield* AiTool;
      const cliConfig = yield* CliConfig;

      if (runtime.consent !== "granted") {
        return noopAnalytics;
      }

      const client = new PostHog(cliConfig.telemetryPosthogKey, {
        host: cliConfig.telemetryPosthogHost,
        flushAt: 1,
        flushInterval: 0,
      });
      // Bounded shutdown so CLI exit isn't blocked on a slow PostHog
      // endpoint. Mirrors supabase analytics.layer.ts. _shutdown(ms) is
      // the timeout-bound variant; client.shutdown() can hang for the
      // default flush interval if the endpoint is unreachable.
      yield* Effect.addFinalizer(() =>
        Effect.promise(() => client._shutdown(5_000)).pipe(Effect.ignore),
      );

      const baseProperties = stripUndefined({
        platform: "cli",
        schema_version: 1,
        device_id: runtime.deviceId,
        $session_id: runtime.sessionId,
        is_first_run: runtime.isFirstRun,
        is_tty: runtime.isTty,
        is_ci: runtime.isCi,
        ai_tool: resolveAiToolName(aiTool.name, runtime),
        os: runtime.os,
        arch: runtime.arch,
        cli_version: runtime.cliVersion,
      });

      const capture = (event: string, properties: Record<string, unknown> = {}) =>
        Effect.gen(function* () {
          const context = yield* CurrentAnalyticsContext;
          const groups = resolveGroups(context);
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
        });

      const identify = (
        distinctId: string,
        properties: Record<string, unknown> = {},
      ) =>
        Effect.sync(() => {
          client.identify({
            distinctId,
            properties: stripUndefined({
              cli_version: runtime.cliVersion,
              os: runtime.os,
              arch: runtime.arch,
              ...properties,
            }),
          });
        });

      const alias = (distinctId: string, aliasValue: string) =>
        Effect.sync(() => {
          client.alias({ distinctId, alias: aliasValue });
        });

      const groupIdentify = (
        groupType: string,
        groupKey: string,
        properties: Record<string, unknown> = {},
      ) =>
        Effect.gen(function* () {
          const context = yield* CurrentAnalyticsContext;
          client.groupIdentify({
            groupType,
            groupKey,
            distinctId: context.distinct_id ?? runtime.distinctId ?? runtime.deviceId,
            properties: stripUndefined(properties),
          });
        });

      return Analytics.of({
        capture,
        identify,
        alias,
        groupIdentify,
      });
    }),
  ).pipe(Layer.provide(telemetryRuntimeLayer), Layer.provide(aiToolLayer));

export const analyticsInternals = {
  stripUndefined,
  contextProperties,
  resolveGroups,
  resolveAiToolName,
} as const;
