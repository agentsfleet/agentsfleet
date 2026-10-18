import { z } from "zod";
import { AGENT_TRIGGER_TYPE } from "@/lib/types";

export const FleetTriggerSchema = z.discriminatedUnion("type", [
  z.object({type: z.literal(AGENT_TRIGGER_TYPE.cron), schedule: z.string(), timezone: z.string().optional()}),
  z.object({type: z.literal(AGENT_TRIGGER_TYPE.webhook), source: z.string(), events: z.array(z.string()).optional()}),
  z.object({type: z.literal(AGENT_TRIGGER_TYPE.mention), source: z.string(), channels: z.array(z.string())}),
]);
