// NDJSON span exporter. Mirrors
// ~/Projects/oss/cli/apps/cli/src/shared/telemetry/exporters/ndjson.ts.
//
// Writes one JSON line per completed span to <tracesDir>/<UTC-date>.ndjson,
// rotated daily, retained for RETENTION_DAYS. Sync writes via
// appendFileSync — span end is on the hot path for command exit, and
// the write target is small (~200 bytes per line) on local disk.
//
// All write errors are swallowed: tracing must never block CLI UX.

import { appendFileSync, mkdirSync, readdirSync, rmSync } from "node:fs";
import path from "node:path";
import type { Tracer } from "effect";

const RETENTION_DAYS = 7;
const MS_PER_DAY = 24 * 60 * 60 * 1000;
const NS_PER_MS = 1_000_000n;

export function initNdjsonExporter(tracesDir: string): void {
  try {
    mkdirSync(tracesDir, { recursive: true, mode: 0o700 });
    const cutoff = Date.now() - RETENTION_DAYS * MS_PER_DAY;
    for (const file of readdirSync(tracesDir)) {
      if (!file.endsWith(".ndjson")) continue;
      const dateStr = file.replace(".ndjson", "");
      const fileDate = new Date(dateStr).getTime();
      if (!Number.isNaN(fileDate) && fileDate < cutoff) {
        rmSync(path.join(tracesDir, file), { force: true });
      }
    }
  } catch {
    // ignore — tracer init is best-effort
  }
}

export function exportSpanToNdjson(span: Tracer.Span, tracesDir: string): void {
  const status = span.status;
  if (status._tag !== "Ended") return;

  const durationMs = Number(status.endTime - status.startTime) / Number(NS_PER_MS);
  const timestampMs = Number(status.startTime / NS_PER_MS);

  const attributes: Record<string, unknown> = {};
  for (const [key, value] of span.attributes) {
    attributes[key] = value;
  }

  let errorCode: string | undefined;
  if (status.exit._tag !== "Success") {
    const exitStr = JSON.stringify(status.exit);
    const match = exitStr.match(/"_tag"\s*:\s*"([^"]+)"/);
    if (match) errorCode = match[1];
  }

  const line = JSON.stringify({
    timestamp: new Date(timestampMs).toISOString(),
    traceId: span.traceId,
    spanId: span.spanId,
    name: span.name,
    duration_ms: Math.round(durationMs),
    status: status.exit._tag === "Success" ? "ok" : "error",
    ...(errorCode !== undefined ? { error_code: errorCode } : {}),
    attributes,
  });

  try {
    const date = new Date().toISOString().split("T")[0];
    appendFileSync(path.join(tracesDir, `${date}.ndjson`), `${line}\n`);
  } catch {
    // ignore write errors
  }
}
