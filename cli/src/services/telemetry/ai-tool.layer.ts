// aiToolLayer — resolves the agent (Claude Code, Cursor, etc.) wrapping
// this CLI invocation via @vercel/detect-agent (env-var inspection of
// well-known agent signals). Mirrors supabase
// apps/cli/src/shared/telemetry/ai-tool.layer.ts; adds a 250ms timeout
// so a hung determineAgent never blocks CLI startup.

import { determineAgent } from "@vercel/detect-agent";
import { Effect, Layer, Option } from "effect";
import { AiTool } from "./ai-tool.service.ts";

const DETECT_TIMEOUT_MS = 250;

function normalizeAgentName(name: string): string {
  return name.replace(/-/g, "_");
}

const noneTool = AiTool.of({ name: Option.none() });

export const aiToolLayer = Layer.effect(
  AiTool,
  // tryPromise turns a rejected promise into a typed Effect error
  // (rather than a defect from Effect.promise), so the catch below
  // intercepts uniformly. Defects from determineAgent's own internals
  // are still caught via catchAllCause to keep startup robust.
  Effect.tryPromise({
    try: () => determineAgent(),
    catch: (cause) => cause,
  }).pipe(
    Effect.timeoutOption(`${DETECT_TIMEOUT_MS} millis`),
    Effect.map((maybe) =>
      Option.match(maybe, {
        onNone: () => noneTool,
        onSome: (result) =>
          AiTool.of({
            name: result.isAgent
              ? Option.some(normalizeAgentName(result.agent.name))
              : Option.none(),
          }),
      }),
    ),
    Effect.catchCause(() => Effect.succeed(noneTool)),
  ),
);
