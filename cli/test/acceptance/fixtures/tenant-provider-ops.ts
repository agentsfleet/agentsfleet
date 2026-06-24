/**
 * Owned helpers for the tenant-provider mutation spec.
 *
 * Tenant provider posture is TENANT-scoped shared state in the DEV tenant —
 * there is no per-run prefix to isolate it the way fleets get
 * `ACCEPTANCE_RUN_PREFIX`. The only safe contract is: capture the baseline
 * before mutating, and restore it (PUT back to the captured self-managed
 * config, or DELETE back to the platform default) in `afterAll` even on
 * failure. These helpers wrap `tenant provider show` so the spec can snapshot
 * and compare without re-implementing the JSON probe in each test.
 *
 * The CLI surface is confirmed against `cli/src/program/cli-tree.ts`:
 *   tenant provider show            (GET  /v1/tenants/me/provider)
 *   tenant provider add --credential <name> [--model <name>]
 *                                   (PUT, mode=self_managed)
 *   tenant provider delete          (DELETE → platform default)
 *
 * JSON keys mirror `ProviderResponse` in `cli/src/commands/tenant.ts`:
 *   mode, provider, model, context_cap_tokens, credential_ref,
 *   synthesised_default, error.
 */

import assert from "node:assert/strict";

import { runFleetctl } from "./cli.js";
import type { RunResult } from "./cli.js";
import { assertNoSecretLeak } from "./negatives.ts";
import { OPENAI_COMPATIBLE_PROVIDER } from "../../../src/constants/custom-endpoint.ts";

type Env = Readonly<Record<string, string>>;

// Custom-endpoint credential argv heads — the typed credential-add form
// (`--provider openai-compatible --base-url <url> --api-key <key>`). UFS: the
// flag literals live once here, reused by the provider-set scenario.
const CMD_CREDENTIAL = "credential" as const;
const SUB_ADD = "add" as const;
const SUB_DELETE = "delete" as const;
export const FLAG_PROVIDER = "--provider" as const;
export const FLAG_BASE_URL = "--base-url" as const;
export const FLAG_API_KEY = "--api-key" as const;

export interface CustomCredentialOptions {
  readonly name: string;
  readonly baseUrl: string;
  readonly apiKey: string;
}

/**
 * `credential add <name> --provider openai-compatible --base-url <url>
 * --api-key <key> --json`. Stores a custom OpenAI-compatible credential so a
 * subsequent `tenant provider add --credential <name>` can target it. Returns
 * the raw run result; the secret-leak regression fires against the JWT.
 */
export async function addCustomEndpointCredential(
  env: Env,
  sessionJwt: string,
  opts: CustomCredentialOptions,
): Promise<RunResult> {
  const result = await runFleetctl(
    [
      CMD_CREDENTIAL, SUB_ADD, opts.name,
      FLAG_PROVIDER, OPENAI_COMPATIBLE_PROVIDER,
      FLAG_BASE_URL, opts.baseUrl,
      FLAG_API_KEY, opts.apiKey,
      FLAG_JSON,
    ],
    { env },
  );
  assertNoSecretLeak(result, sessionJwt);
  // The api_key must never echo to stdout/stderr (VLT).
  assert.ok(
    !`${result.stdout}\n${result.stderr}`.includes(opts.apiKey),
    "custom-endpoint credential api_key leaked into CLI output",
  );
  return result;
}

/** Best-effort delete of a named credential (afterAll cleanup). */
export async function deleteCredentialByName(env: Env, name: string): Promise<void> {
  try {
    await runFleetctl([CMD_CREDENTIAL, SUB_DELETE, name, FLAG_JSON], { env });
  } catch {
    /* best-effort teardown — never throw out of afterAll */
  }
}

// Provider-mode wire literals — mirror `PROVIDER_MODE` in
// `cli/src/constants/billing.ts`, which itself mirrors
// `src/state/tenant_provider.zig` (`Mode`). UFS: named once, reused.
export const TENANT_PROVIDER_MODE = {
  platform: "platform",
  selfManaged: "self_managed",
} as const;

// Subcommand argv heads — every occurrence is the same literal, so UFS
// pins them here rather than scattering string arrays through the spec.
// The shared `tenant provider` base is hoisted once and spread into each
// command so neither head literal is repeated (UFS: no string used twice).
const TENANT_PROVIDER_BASE: ReadonlyArray<string> = ["tenant", "provider"];
const SUBCOMMAND_SHOW = "show" as const;
const SUBCOMMAND_ADD = "add" as const;
const SUBCOMMAND_DELETE = "delete" as const;
export const TENANT_PROVIDER_SHOW_ARGS: ReadonlyArray<string> = [...TENANT_PROVIDER_BASE, SUBCOMMAND_SHOW];
export const TENANT_PROVIDER_ADD_ARGS: ReadonlyArray<string> = [...TENANT_PROVIDER_BASE, SUBCOMMAND_ADD];
export const TENANT_PROVIDER_DELETE_ARGS: ReadonlyArray<string> = [...TENANT_PROVIDER_BASE, SUBCOMMAND_DELETE];

export const FLAG_JSON = "--json" as const;
export const FLAG_CREDENTIAL = "--credential" as const;
export const FLAG_MODEL = "--model" as const;

// Backend surfaces an unresolved credential via the `error` field rather
// than failing the PUT — the posture row is still written. The spec
// tolerates this because no CLI command seeds a named vault credential;
// the load-bearing assertion is the `mode` transition, not key resolution.
export const CREDENTIAL_MISSING_ERROR = "credential_missing" as const;

export interface ProviderSnapshot {
  readonly mode?: string;
  readonly provider?: string;
  readonly model?: string;
  readonly context_cap_tokens?: number;
  readonly credential_ref?: string | null;
  readonly synthesised_default?: boolean;
  readonly error?: string;
}

function parseProvider(stdout: string): ProviderSnapshot {
  const trimmed = stdout.trim();
  assert.ok(trimmed.length > 0, "tenant provider show --json produced empty stdout");
  return JSON.parse(trimmed) as ProviderSnapshot;
}

/**
 * `tenant provider show --json` → parsed snapshot. Asserts a clean exit and
 * runs the secret-leak regression against the supplied JWT.
 */
export async function showProvider(env: Env, sessionJwt: string): Promise<ProviderSnapshot> {
  const result = await runFleetctl([...TENANT_PROVIDER_SHOW_ARGS, FLAG_JSON], { env });
  assertNoSecretLeak(result, sessionJwt);
  assert.equal(result.code, 0, `tenant provider show exited ${result.code}: ${result.stderr}`);
  return parseProvider(result.stdout);
}

export interface AddProviderOptions {
  readonly credential: string;
  readonly model?: string | undefined;
}

/**
 * `tenant provider add --credential <name> [--model <name>] --json`. Returns
 * the raw run result so the caller can branch on exit code (the backend may
 * accept the PUT and report `credential_missing`, or reject an unknown
 * credential outright — both are legitimate and the spec handles each).
 */
export async function addProvider(
  env: Env,
  sessionJwt: string,
  opts: AddProviderOptions,
): Promise<RunResult> {
  const args = [...TENANT_PROVIDER_ADD_ARGS, FLAG_CREDENTIAL, opts.credential];
  if (opts.model) args.push(FLAG_MODEL, opts.model);
  args.push(FLAG_JSON);
  const result = await runFleetctl(args, { env });
  assertNoSecretLeak(result, sessionJwt);
  return result;
}

/**
 * `tenant provider delete --json` → parsed snapshot of the post-reset state.
 * Asserts a clean exit; the reset always lands on the platform default.
 */
export async function deleteProvider(env: Env, sessionJwt: string): Promise<ProviderSnapshot> {
  const result = await runFleetctl([...TENANT_PROVIDER_DELETE_ARGS, FLAG_JSON], { env });
  assertNoSecretLeak(result, sessionJwt);
  assert.equal(result.code, 0, `tenant provider delete exited ${result.code}: ${result.stderr}`);
  return parseProvider(result.stdout);
}

/**
 * Assert a snapshot is a well-formed self-managed posture: mode flipped,
 * credential_ref echoes the supplied name, not flagged as the synthesised
 * default, and any `error` field is exactly the known credential-missing
 * marker (anything else is a resolver regression). Keeps the spec's `add`
 * test body inside the 50-line function bound.
 */
export function assertSelfManagedSnapshot(
  after: ProviderSnapshot,
  expectedCredentialRef: string,
): void {
  assert.equal(
    after.mode,
    TENANT_PROVIDER_MODE.selfManaged,
    `expected mode=${TENANT_PROVIDER_MODE.selfManaged} after add; got ${JSON.stringify(after)}`,
  );
  assert.equal(
    after.credential_ref,
    expectedCredentialRef,
    `credential_ref should echo the supplied name; got ${JSON.stringify(after)}`,
  );
  assert.notEqual(
    after.synthesised_default,
    true,
    "a self-managed posture must not be flagged as the synthesised platform default",
  );
  if (typeof after.error === "string" && after.error.length > 0) {
    assert.equal(
      after.error,
      CREDENTIAL_MISSING_ERROR,
      `unexpected provider error field: ${after.error}`,
    );
  }
}

/**
 * Assert a non-zero `add` result is a recognised upstream rejection and that
 * the live posture still equals the captured baseline (a rejected add must
 * not partially mutate). Returns nothing; throws via assert on violation.
 */
export async function assertRejectedAddLeftBaseline(
  env: Env,
  sessionJwt: string,
  added: RunResult,
  baselineMode: string | undefined,
): Promise<void> {
  assert.match(
    `${added.stderr}\n${added.stdout}`,
    /credential|not found|HTTP_4\d\d|UZ-|invalid/i,
    `add failed with an unexpected error shape: ${added.stderr || added.stdout}`,
  );
  const stillBaseline = await showProvider(env, sessionJwt);
  assert.equal(
    stillBaseline.mode,
    baselineMode,
    "rejected add must leave the baseline posture untouched",
  );
}

/**
 * Best-effort baseline restore for `afterAll`. If the captured baseline was a
 * self-managed posture, PUT it back; otherwise DELETE to the platform
 * default. Swallows every error — teardown must never mask a test failure.
 */
export async function restoreProviderBaseline(
  env: Env,
  sessionJwt: string,
  baseline: ProviderSnapshot,
): Promise<void> {
  try {
    if (baseline.mode === TENANT_PROVIDER_MODE.selfManaged && baseline.credential_ref) {
      await addProvider(env, sessionJwt, {
        credential: baseline.credential_ref,
        model: baseline.model,
      });
      return;
    }
    await runFleetctl([...TENANT_PROVIDER_DELETE_ARGS, FLAG_JSON], { env });
  } catch {
    /* best-effort teardown — shared tenant left on platform default at worst */
  }
}
