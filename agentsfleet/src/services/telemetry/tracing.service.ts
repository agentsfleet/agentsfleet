// Tracing — re-export Effect's Tracer service tag as the canonical
// tracing boundary for the CLI. Mirrors
// ~/Projects/oss/cli/apps/cli/src/shared/telemetry/tracing.service.ts.
//
// CLI policy (consent, identity, exporter wiring, global attributes)
// lives in tracing.layer.ts. Commands and the dispatcher depend on the
// Tracer service only; they cannot reach into exporter internals.

import { Tracer } from "effect";

export const Tracing = Tracer.Tracer;
