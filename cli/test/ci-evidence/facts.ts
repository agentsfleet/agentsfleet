export const id = (value: number): string => `019a0000-0000-7000-8000-${value.toString(16).padStart(12, "0")}`;
export const SOURCE = "a".repeat(40);
export const BASE = "b".repeat(40);
export const HEAD = "c".repeat(40);
export const WORKSPACE = id(1);
export const RUNNER = id(40);
export const REPOSITORY = "agentsfleet/linkwarden";
export const DRAFT_URL = `https://github.com/${REPOSITORY}/pull/9`;
export const LOG_URL = `https://api.github.com/repos/${REPOSITORY}/actions/jobs/88/logs`;
export const LOG_QUOTE = "Log says synthetic failure";
export const LOG = `Status: 200\nContent-Type: text/plain\n\nResponse Body:\n${LOG_QUOTE}`;
export const TRUNCATED_LOG = "Status: 200\nContent-Type: text/plain\n[Response truncated at 1048576 bytes]\n\nResponse Body:\nRequired source unavailable";
export const BEFORE = "2026-11-01T00:00:00Z";
export const AFTER = "2026-11-01T03:00:00Z";
export const MODEL = {provider: "synthetic-provider", id: "synthetic-model", version: null,
  settings: {inference: {api: "chat", base_url: "https://model.example.test", max_output_tokens: 128}, context_cap_tokens: 4096, context: null}};
