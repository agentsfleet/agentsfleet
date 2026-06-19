/**
 * Concurrency-slice helpers — owned solely by concurrency.spec.ts.
 *
 * The shared acceptance fixtures (cli.js, seed.ts, teardown.ts, …) are
 * off-limits for edits, so the concurrency-specific primitives live here:
 *   - boundedAll: run a list of async thunks with a hard concurrency cap
 *     (`Promise.all` over the whole batch is the explicit goal, but the cap
 *     lets a caller tighten it under a constrained runner without rewriting
 *     the spec).
 *   - seedCredentialsFile / readCredentialsRaw: pre-write a well-formed
 *     credentials.json into a shared state dir and read its raw bytes back,
 *     so the spec can prove concurrent reads never corrupt it.
 *   - parseAllJson: assert every captured stdout parses as JSON (cross-talk
 *     / interleaving would surface as a parse failure or a wrong shape).
 *
 * RULE UFS: every wire-format literal that appears here or in the spec is a
 * named const exported from this module.
 */

import fs from "node:fs/promises";
import path from "node:path";

import type { RunResult } from "./cli.js";

// credentials.json schema keys (mirrors src/lib/state.ts loadCredentials
// fallback). A drift here surfaces as a failed round-trip assertion.
export const CREDENTIALS_FILE = "credentials.json";
export const CREDENTIALS_FILE_MODE = 0o600;
export const CREDENTIALS_KEY_TOKEN = "token";
export const CREDENTIALS_KEY_SAVED_AT = "saved_at";
export const CREDENTIALS_KEY_SESSION_ID = "session_id";
export const CREDENTIALS_KEY_API_URL = "api_url";

// `agent list --json` envelope key carrying the row array.
export const LIST_ITEMS_KEY = "items";
// `doctor --json` envelope key carrying the per-check array.
export const DOCTOR_CHECKS_KEY = "checks";

// Fan-out width for the concurrent sweeps. Eight is wide enough to expose a
// file-write race or a shared-state mutation, small enough not to throttle a
// modest CI runner. Bounded so a constrained runner can dial it down.
export const CONCURRENCY_WIDTH = 8;

export interface CredentialsRecord {
  readonly token: string;
  readonly saved_at: number;
  readonly session_id: string | null;
  readonly api_url: string | null;
}

export function credentialsPathFor(stateDir: string): string {
  return path.join(stateDir, CREDENTIALS_FILE);
}

/**
 * Write a well-formed credentials.json into `stateDir` (mode 0600), matching
 * the on-disk schema the CLI reads. Returns the exact bytes written so the
 * caller can assert the file is byte-identical after a concurrent read storm.
 */
export async function seedCredentialsFile(
  stateDir: string,
  record: CredentialsRecord,
): Promise<string> {
  await fs.mkdir(stateDir, { recursive: true });
  const body = `${JSON.stringify(record, null, 2)}\n`;
  await fs.writeFile(credentialsPathFor(stateDir), body, { mode: CREDENTIALS_FILE_MODE });
  return body;
}

export async function readCredentialsRaw(stateDir: string): Promise<string> {
  return fs.readFile(credentialsPathFor(stateDir), "utf8");
}

/**
 * Run `thunks` with at most `width` in flight at once. The default width is
 * the full batch length, i.e. a plain `Promise.all` fan-out; passing a
 * smaller width caps concurrency for a constrained runner. Results preserve
 * input order.
 */
export async function boundedAll<T>(
  thunks: ReadonlyArray<() => Promise<T>>,
  width: number = thunks.length,
): Promise<T[]> {
  if (width <= 0) throw new Error("boundedAll: width must be positive");
  const results: T[] = new Array(thunks.length);
  let next = 0;
  const worker = async (): Promise<void> => {
    for (;;) {
      const index = next;
      next += 1;
      if (index >= thunks.length) return;
      const thunk = thunks[index];
      if (!thunk) return;
      results[index] = await thunk();
    }
  };
  const lane = Math.min(width, thunks.length);
  const workers: Array<Promise<void>> = [];
  for (let i = 0; i < lane; i += 1) workers.push(worker());
  await Promise.all(workers);
  return results;
}

/**
 * Parse every result's stdout as JSON, asserting each is a non-null object.
 * Interleaved / truncated output from a cross-talk bug surfaces here as a
 * JSON.parse throw with the offending index in the message.
 */
export function parseAllJson(results: ReadonlyArray<RunResult>): Array<Record<string, unknown>> {
  return results.map((result, index) => {
    const trimmed = result.stdout.trim();
    let parsed: unknown;
    try {
      parsed = JSON.parse(trimmed);
    } catch (cause) {
      const reason = cause instanceof Error ? cause.message : String(cause);
      throw new Error(`invocation ${index}: stdout is not valid JSON (${reason}): ${trimmed.slice(0, 200)}`);
    }
    if (parsed === null || typeof parsed !== "object") {
      throw new Error(`invocation ${index}: JSON is not an object: ${trimmed.slice(0, 200)}`);
    }
    return parsed as Record<string, unknown>;
  });
}
