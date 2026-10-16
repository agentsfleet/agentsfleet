import type { Definition } from "@/lib/api/draft-types";
import { HTTP_REQUEST } from "./constants";
import { NANOS_PER_USD } from "@/lib/types";

export const EXECUTION_REPLY_PREFIX = "ACK";
export const WEBHOOK_SOURCE_GITHUB = "github";
export const WEBHOOK_EVENT_WORKFLOW_RUN = "workflow_run";

export function fixtureConfig(name: string, tool = HTTP_REQUEST): Definition["config"] {
  return { name, "x-agentsfleet": { triggers: [], tools: [tool], budget: { daily_nanos: String(NANOS_PER_USD) } } };
}

export function fixtureDefinition(name: string, instructions = "Acknowledge the request.",
  config = fixtureConfig(name)): Definition {
  return { schema_version: 1, description: "Acceptance fixture",
    instructions_md: instructions, config };
}

export function executionInstructions(): string {
  return `You are an acceptance probe. When you receive a message, reply with exactly one line: ` +
    `the word ${EXECUTION_REPLY_PREFIX}, a space, then the message text verbatim. ` +
    "Do not call any tool. Do not add anything before or after that line.\n";
}

export function webhookConfig(name: string, credentialName: string): Definition["config"] {
  return { name, "x-agentsfleet": { triggers: [{ type: "webhook", source: WEBHOOK_SOURCE_GITHUB,
    events: [WEBHOOK_EVENT_WORKFLOW_RUN], credential_name: credentialName }], tools: [HTTP_REQUEST],
    budget: { daily_nanos: String(NANOS_PER_USD) } } };
}
