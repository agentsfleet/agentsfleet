/**
 * The provider ids `--provider` accepts on the typed secret form.
 *
 * A mirror of the dial names NullClaw's provider factory recognises
 * (docs/architecture/billing_and_provider_keys.md §9 — provider name →
 * endpoint → wire format), plus the custom-endpoint sentinel imported from
 * custom-endpoint.ts — agentsfleet's own opt-in, not a NullClaw dial target.
 * Mirrored, not fetched: custom-endpoint.ts records why the cli/ Bun project
 * re-states backend literals in exactly one place. Adding a provider is one
 * array entry (RULE CFG); every reader — the flag wiring, the help text, and
 * the tests — imports this catalogue (RULE UFS).
 */

import { OPENAI_COMPATIBLE_PROVIDER } from "./custom-endpoint.ts";

export const PROVIDER_IDS = [
  "anthropic",
  "fireworks",
  "fireworks-ai",
  "groq",
  "kimi",
  "kimi-intl",
  "moonshot",
  "moonshot-intl",
  "openai",
  "openrouter",
  "together",
  "together-ai",
  OPENAI_COMPATIBLE_PROVIDER,
] as const;
