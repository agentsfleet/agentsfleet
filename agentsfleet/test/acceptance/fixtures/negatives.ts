/**
 * Negative-path helpers used across the acceptance specs.
 *
 * Each helper composes a `runAgentctl` call, asserts a non-zero exit,
 * and verifies stderr/stdout shape against the documented contract.
 */

import { runAgentctl } from "./cli.js";
import type { RunResult } from "./cli.js";

type Env = Readonly<Record<string, string>>;

interface ErrorEnvelope {
  error?: { code?: string };
}

export interface InvalidArgEnvelope extends RunResult {
  readonly envelope: ErrorEnvelope;
}

function lastJsonObject(stdout: string): ErrorEnvelope | null {
  const trimmed = stdout.trim();
  if (!trimmed) return null;
  try {
    return JSON.parse(trimmed) as ErrorEnvelope;
  } catch {
    return null;
  }
}

export async function expectInvalidSubcommand(group: string, env: Env): Promise<RunResult> {
  const result = await runAgentctl([group, "pogo"], { env });
  if (result.code === 0) {
    throw new Error(`expected non-zero for "${group} pogo"; got 0; stdout: ${result.stdout}`);
  }
  const stderr = result.stderr.toLowerCase();
  if (!/unknown|invalid|usage|did you mean/.test(stderr)) {
    throw new Error(`expected dispatcher error on stderr for "${group} pogo"; got: ${result.stderr}`);
  }
  return result;
}

export async function expectMissingArg(args: ReadonlyArray<string>, env: Env): Promise<RunResult> {
  const result = await runAgentctl(args, { env });
  if (result.code === 0) {
    throw new Error(`expected non-zero for "${args.join(" ")}"; got 0; stdout: ${result.stdout}`);
  }
  const merged = `${result.stderr}\n${result.stdout}`.toLowerCase();
  if (!/missing|required|usage|expected/.test(merged)) {
    throw new Error(`expected missing-arg stem for "${args.join(" ")}"; got stderr: ${result.stderr}`);
  }
  return result;
}

export async function expectInvalidArgValue(
  args: ReadonlyArray<string>,
  env: Env,
  expectedErrorCode?: string | null,
): Promise<RunResult | InvalidArgEnvelope> {
  const result = await runAgentctl(args, { env });
  if (result.code === 0) {
    throw new Error(`expected non-zero for "${args.join(" ")}"; got 0; stdout: ${result.stdout}`);
  }
  if (!args.includes("--json")) {
    if (expectedErrorCode && !result.stderr.includes(expectedErrorCode)) {
      throw new Error(`expected error code ${expectedErrorCode} on stderr; got: ${result.stderr}`);
    }
    return result;
  }
  const envelope = lastJsonObject(result.stdout);
  if (!envelope || !envelope.error || !envelope.error.code) {
    throw new Error(`expected {error:{code}} envelope on stdout; got: ${result.stdout}`);
  }
  if (expectedErrorCode && envelope.error.code !== expectedErrorCode) {
    throw new Error(`expected error.code ${expectedErrorCode}; got ${envelope.error.code}`);
  }
  return { ...result, envelope };
}

export interface CapturedOutput {
  readonly stderr: string;
  readonly stdout: string;
}

export function assertNoConnectionError(captured: CapturedOutput, args: ReadonlyArray<string>): void {
  const merged = `${captured.stderr}\n${captured.stdout}`;
  if (/ECONNREFUSED|ENOTFOUND|EAI_AGAIN|fetch failed/.test(merged)) {
    throw new Error(
      `client-side validator should have caught "${args.join(" ")}" before any network call — ` +
      `observed network error: ${merged}`,
    );
  }
}

export function assertNoSecretLeak(captured: CapturedOutput, secret: string | null | undefined): void {
  if (!secret) return;
  if (captured.stdout.includes(secret) || captured.stderr.includes(secret)) {
    throw new Error("WS-E #C1 regression: minted JWT leaked into captured stdout/stderr");
  }
}
