// AiTool service — declares the detected coding agent attached to every
// analytics event. Mirrors
// supabase apps/cli/src/shared/telemetry/ai-tool.service.ts.

import { Context, type Option } from "effect";

interface AiToolShape {
  readonly name: Option.Option<string>;
}

export type AiTool = AiToolShape;
export const AiTool = Context.Service<AiTool>(
  "agentsfleet/telemetry/AiTool",
);
