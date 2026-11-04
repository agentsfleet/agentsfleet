import type { Definition } from "@/lib/api/draft-types";
import starters from "@/public/fleet-starters.json" with { type: "json" };

export const DEFAULT_INSTANCE_NAME = "my-fleet";
export const DEFAULT_DAILY_NANOS = "1000000000";
export const CHAT_PLACEHOLDER = "Describe the work you want this fleet to do…";
export const CHAT_STARTERS: readonly {title: string; prompt: string}[] = starters;

export function initialDefinition(): Definition {
  return {schema_version: 1, description: "A fleet created in chat",
    instructions_md: "", config: {name: DEFAULT_INSTANCE_NAME,
      "x-agentsfleet": {triggers: [], tools: [], budget: {daily_nanos: DEFAULT_DAILY_NANOS}}}};
}

export function safeActionHref(value: string | undefined): string | undefined {
  return value?.startsWith("/") && !value.startsWith("//") && !value.includes("\\") ? value : undefined;
}
