// tracingLayer — CLI tracing implementation. Mirrors
// ~/Projects/oss/cli/apps/cli/src/shared/telemetry/tracing.layer.ts.
//
// Owns telemetry bootstrap (consent evaluation already done in
// runtime.layer.ts), exporter wiring, and global span attributes.
// Commands depend on the `Tracing` service tag only; they cannot
// reach into exporter internals.
//
// Both exporters are gated by TelemetryRuntime.consent / showDebug
// before spans start flowing. Debug exporter writes to stderr directly
// (process.stderr.write) — Supabase routes through a Stdio service;
// agentsfleet's writes-to-stderr convention matches the rest of the CLI
// where output.error → process.stderr.

import { Effect, Layer, Option, Tracer, type Exit, type Context } from "effect";
import { makeDebugConsoleExporter } from "./exporters/debug-console.ts";
import { exportSpanToNdjson, initNdjsonExporter } from "./exporters/ndjson.ts";
import { TelemetryRuntime } from "./runtime.service.ts";
import { Tracing } from "./tracing.service.ts";

const HEX_CHARS = "0123456789abcdef";
const CONSENT_GRANTED = "granted" as const;

function generateHexId(length: number): string {
  let result = "";
  for (let i = 0; i < length; i++) {
    result += HEX_CHARS[Math.floor(Math.random() * HEX_CHARS.length)];
  }
  return result;
}

class ExportableSpan implements Tracer.Span {
  readonly _tag = "Span" as const;
  readonly spanId: string;
  readonly traceId: string;
  readonly sampled: boolean;
  readonly name: string;
  readonly parent: Option.Option<Tracer.AnySpan>;
  readonly annotations: Context.Context<never>;
  readonly links: ReadonlyArray<Tracer.SpanLink>;
  readonly kind: Tracer.SpanKind;

  status: Tracer.SpanStatus;
  attributes: Map<string, unknown> = new Map();

  private readonly onEnd: (span: ExportableSpan) => void;

  constructor(
    options: {
      readonly name: string;
      readonly parent: Option.Option<Tracer.AnySpan>;
      readonly annotations: Context.Context<never>;
      readonly links: Array<Tracer.SpanLink>;
      readonly startTime: bigint;
      readonly kind: Tracer.SpanKind;
      readonly sampled: boolean;
    },
    onEnd: (span: ExportableSpan) => void,
  ) {
    this.name = options.name;
    this.parent = options.parent;
    this.annotations = options.annotations;
    this.links = options.links;
    this.kind = options.kind;
    this.sampled = options.sampled;
    this.status = { _tag: "Started", startTime: options.startTime };
    this.traceId = Option.match(options.parent, {
      onNone: () => generateHexId(32),
      onSome: (parent) => parent.traceId,
    });
    this.spanId = generateHexId(16);
    this.onEnd = onEnd;
  }

  end(endTime: bigint, exit: Exit.Exit<unknown, unknown>): void {
    this.status = {
      _tag: "Ended",
      startTime: this.status.startTime,
      endTime,
      exit,
    };
    this.onEnd(this);
  }

  attribute(key: string, value: unknown): void {
    this.attributes.set(key, value);
  }

  event(_name: string, _startTime: bigint, _attributes?: Record<string, unknown>): void {}

  addLinks(_links: ReadonlyArray<Tracer.SpanLink>): void {}
}

export const tracingLayer = Layer.effect(
  Tracing,
  Effect.gen(function* () {
    const telemetryRuntime = yield* TelemetryRuntime;
    const exportSpanToDebugConsole = makeDebugConsoleExporter((line) => {
      process.stderr.write(line);
    });

    if (telemetryRuntime.consent === CONSENT_GRANTED) {
      initNdjsonExporter(telemetryRuntime.tracesDir);
    }

    function onSpanEnd(span: ExportableSpan): void {
      if (!span.sampled) return;
      if (telemetryRuntime.consent === CONSENT_GRANTED) {
        exportSpanToNdjson(span, telemetryRuntime.tracesDir);
      }
      if (telemetryRuntime.showDebug) {
        exportSpanToDebugConsole(span);
      }
    }

    const globalAttrs: Record<string, unknown> = {
      schema_version: 1,
      device_id: telemetryRuntime.deviceId,
      session_id: telemetryRuntime.sessionId,
      is_first_run: telemetryRuntime.isFirstRun,
      is_tty: telemetryRuntime.isTty,
      is_ci: telemetryRuntime.isCi,
      os: telemetryRuntime.os,
      arch: telemetryRuntime.arch,
      cli_version: telemetryRuntime.cliVersion,
    };

    return Tracer.make({
      span(options) {
        const span = new ExportableSpan(options, onSpanEnd);
        for (const [key, value] of Object.entries(globalAttrs)) {
          span.attribute(key, value);
        }
        return span;
      },
    });
  }),

);
