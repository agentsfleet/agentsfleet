import { z } from "zod";
import { AGENT_TRIGGER_TYPE } from "@/lib/types";

const TRIGGER_ERROR = "Triggers must be webhook, cron or mention.";
const WEBHOOK_SOURCE_ERROR = "Webhook source must use lowercase letters, digits or hyphens; approval is reserved.";
const WEBHOOK_VERIFIER_ERROR = "Webhook source must be github, linear or slack, or declare signature.secret_ref for Svix verification.";
const BUILTIN_SOURCES = new Set(["github", "linear", "slack"]);
const ConfigTriggersSchema = z.object({
  "x-agentsfleet": z.object({triggers: z.array(z.object({type: z.enum(AGENT_TRIGGER_TYPE)}).passthrough()).optional()}).optional(),
});
const WebhookSchema = z.object({
  source: z.string().min(1).refine((source) => !/[^a-z0-9-]/.test(source) && source !== "approval", WEBHOOK_SOURCE_ERROR),
  signature: z.object({secret_ref: z.string().min(1)}).strict().optional(),
}).refine((trigger) => BUILTIN_SOURCES.has(trigger.source) || trigger.signature !== undefined, WEBHOOK_VERIFIER_ERROR);

export function draftTriggerError(config: Record<string, unknown>): string | null {
  const parsed = ConfigTriggersSchema.safeParse(config);
  if (!parsed.success) return TRIGGER_ERROR;
  for (const trigger of parsed.data["x-agentsfleet"]?.triggers ?? []) {
    if (trigger.type !== AGENT_TRIGGER_TYPE.webhook) continue;
    const webhook = WebhookSchema.safeParse(trigger);
    if (!webhook.success) return `Webhook: ${webhook.error.issues.map((issue) => issue.message).join(" ")}`;
  }
  return null;
}
