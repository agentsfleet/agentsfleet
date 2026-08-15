/**
 * The provider ids `--provider` accepts on the typed secret form.
 *
 * A mirror of what the vendored NullClaw runtime can actually dial, which is
 * decided by one function — `classifyProvider` in
 * `zig-pkg/nullclaw-<version>/src/providers/factory.zig`. That function reads
 * three tables, so this catalogue does too: the `compat_providers` dial table
 * (the file's own "single source of truth for all OpenAI-compatible
 * providers"), the `core_providers` map of natively-implemented providers, and
 * the alias arms of `canonicalProviderName` in `provider_names.zig`. Authoring
 * the list from the architecture doc's illustrative table is how it silently
 * narrowed to 13 ids and began rejecting deepseek/cerebras/mistral.
 *
 * `providers-parity.unit.test.ts` re-extracts all three blocks from the
 * vendored source and fails on any drift, so bumping the NullClaw dependency
 * either updates this list or fails the suite — it never silently narrows a
 * public flag. Mirrored, not fetched: custom-endpoint.ts records why the cli/
 * Bun project re-states backend literals in exactly one place.
 *
 * Two deliberate departures from the dial set, both enforced by that test:
 *   - `openai-compatible` is carried and is NOT a NullClaw name. It is
 *     agentsfleet's own opt-in sentinel for a user-supplied endpoint.
 *   - CLI_ENGINE_PROVIDERS are dialable by NullClaw but rejected here. They
 *     spawn a local coding-agent binary and carry no API key, so storing one
 *     as a key-bearing credential produces a secret that can never dial.
 *
 * Adding a provider is one array entry (RULE CFG); every reader — the flag
 * wiring, the help text, and the tests — imports this catalogue (RULE UFS).
 */

import { OPENAI_COMPATIBLE_PROVIDER } from "./custom-endpoint.ts";

/**
 * Providers NullClaw implements by spawning a local CLI (`claude`, `codex`,
 * `gemini`) rather than dialing an HTTP endpoint with a key. They authenticate
 * through that binary's own session, so they belong to a future engine surface,
 * not to a credential that stores an api_key. Recognised on input purely so the
 * rejection can say why instead of reading as an unknown name.
 */
export const CLI_ENGINE_PROVIDERS = [
  "claude-cli",
  "claude-code",
  "codex-cli",
  "gemini-cli",
  "openai-codex",
] as const;

export const CLI_ENGINE_REJECTION =
  "spawns a local CLI and carries no API key, so it cannot back a stored credential yet";

// The ids help text names outright — declared once, referenced from both the
// catalogue and PROVIDER_EXAMPLES (RULE UFS).
const EXAMPLE_ANTHROPIC = "anthropic";
const EXAMPLE_OPENAI = "openai";
const EXAMPLE_DEEPSEEK = "deepseek";
const EXAMPLE_GROQ = "groq";

export const PROVIDER_IDS = [
  "aihubmix",
  EXAMPLE_ANTHROPIC,
  "ark",
  "astrai",
  "atlas",
  "atlas-cloud",
  "atlascloud",
  "aws-bedrock",
  "azure",
  "azure-openai",
  "azure_openai",
  "baichuan",
  "baidu",
  "bedrock",
  "bigmodel",
  "build.nvidia.com",
  "byteplus",
  "byteplus-plan",
  "cerebras",
  "chutes",
  "cloudflare",
  "cloudflare-ai",
  "cohere",
  "copilot",
  "dashscope",
  "dashscope-intl",
  "dashscope-us",
  EXAMPLE_DEEPSEEK,
  "doubao",
  "evolink",
  "fireworks",
  "fireworks-ai",
  "gemini",
  "github-copilot",
  "glm",
  "glm-cn",
  "glm-global",
  "google",
  "google-gemini",
  "google-vertex",
  "grok",
  EXAMPLE_GROQ,
  "huggingface",
  "hunyuan",
  "kimi",
  "kimi-cn",
  "kimi-code",
  "kimi-global",
  "kimi-intl",
  "kimi_coding",
  "litellm",
  "llama.cpp",
  "llamacpp",
  "lm-studio",
  "lmstudio",
  "mimo",
  "minimax",
  "minimax-cn",
  "minimax-global",
  "minimax-intl",
  "minimax-io",
  "minimaxi",
  "mistral",
  "moonshot",
  "moonshot-cn",
  "moonshot-global",
  "moonshot-intl",
  "nearai",
  "novita",
  "novita-ai",
  "nvidia",
  "nvidia-nim",
  "ollama",
  EXAMPLE_OPENAI,
  "opencode",
  "opencode-zen",
  "openrouter",
  "osaurus",
  "ovh",
  "ovhcloud",
  "perplexity",
  "poe",
  "qianfan",
  "qwen",
  "qwen-intl",
  "qwen-portal",
  "qwen-us",
  "sglang",
  "shengsuanyun",
  "siliconflow",
  "synthetic",
  "telnyx",
  "tencent",
  "together",
  "together-ai",
  "venice",
  "vercel",
  "vercel-ai",
  "vertex",
  "vertex-ai",
  "vllm",
  "volcengine",
  "volcengine-plan",
  "xai",
  "xiaomi",
  "xiaomi-mimo",
  "z.ai",
  "z.ai-cn",
  "z.ai-global",
  "zai",
  "zai-cn",
  "zai-global",
  "zhipu",
  "zhipu-cn",
  "zhipu-global",
  OPENAI_COMPATIBLE_PROVIDER,
] as const;

export type ProviderId = (typeof PROVIDER_IDS)[number];

/**
 * The handful of ids `--help` names outright. The full catalogue is two orders
 * of magnitude too long for a flag description — inlining all of it buries
 * every other option on the command. An unknown value still prints the whole
 * accepted set, which is where an exhaustive list is actually useful.
 * `providers-parity.unit.test.ts` pins every example to a real member.
 */
export const PROVIDER_EXAMPLES = [
  EXAMPLE_ANTHROPIC,
  EXAMPLE_OPENAI,
  EXAMPLE_DEEPSEEK,
  EXAMPLE_GROQ,
] as const;
