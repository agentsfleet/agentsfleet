// Shared fixtures for the `secret create` integration suites.
//
// Extracted when custom-secret-create.integration.test.ts passed the
// repository's 350-line cap.

import { withAuthedStateDir } from "./helpers-cli-state.ts";

// The accepted `--provider` set is now whatever GET /v1/models serves, so these
// tests state it as catalogue rows rather than importing a compiled-in list.
// Two providers is enough to prove membership, non-membership, and case-folding
// while keeping the rejection message short enough to assert on.
import { catalogueModel, cataloguePage } from "./helpers-catalogue.ts";
const ANTHROPIC_CONTEXT_TOKENS = 1_000_000;
export const CATALOGUE_PAGE = cataloguePage([
  catalogueModel("claude-opus-5", "anthropic", { context_cap_tokens: ANTHROPIC_CONTEXT_TOKENS }),
  catalogueModel("gpt-5.6-sol", "openai", { context_cap_tokens: 1050000, output_nanos_per_mtok: "30000000000" }),
]);

export const WS_ID = "ws_custom_cred_test";
export const SECRET_NAME = "vllm-gateway";
export const VALID_BASE_URL = "https://vllm.corp.example/v1";
export const API_KEY = "sk-custom-secret-do-not-log";
export const MODEL = "qwen2.5-coder";
// A model the CATALOGUE_PAGE fixture actually serves. `--model` is now checked
// against the catalogue too, so a test proving PROVIDER folding must not trip
// the model check on its way there.
export const CATALOGUE_MODEL = "claude-opus-5";
export const NON_HTTPS_BASE_URL = "http://vllm.corp.example/v1";

export const authedScope = <T>(fn: (stateDir: string) => Promise<T>): Promise<T> =>
  withAuthedStateDir({ workspaceId: WS_ID, sessionId: "sess_custom_cred" }, fn);
