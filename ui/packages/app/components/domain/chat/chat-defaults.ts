import type { Definition } from "@/lib/api/draft-types";

export const DEFAULT_INSTANCE_NAME = "my-fleet";
export const DEFAULT_DAILY_NANOS = "1000000000";
export const CHAT_PLACEHOLDER = "Describe the work you want this fleet to do…";
export const EDIT_SETUP_PLACEHOLDER = "What would you like to change?";
export const CHAT_EXAMPLES = [
  "Review GitHub pull requests and share findings in Slack.",
  "Watch production errors and prepare a repair for review.",
  "Summarize the work my team should act on each morning.",
] as const;

export function initialDefinition(): Definition {
  return {schema_version: 1, description: "A fleet created in chat",
    instructions_md: "", config: {name: DEFAULT_INSTANCE_NAME,
      "x-agentsfleet": {triggers: [], tools: [], budget: {daily_nanos: DEFAULT_DAILY_NANOS}}}};
}

export function safeActionHref(value: string | undefined): string | undefined {
  return value?.startsWith("/") && !value.startsWith("//") && !value.includes("\\") ? value : undefined;
}
