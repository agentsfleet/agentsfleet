/**
 * Shared lifecycle action helpers — stop / resume / kill / expectStatus.
 *
 * Each helper composes a `runZombiectl` call, asserts exit 0, and
 * (for status) returns the parsed JSON envelope.
 */

import { runZombiectl } from "./cli.js";

type Env = Readonly<Record<string, string>>;

export interface ZombieRow {
  readonly id?: string;
  readonly zombie_id?: string;
  readonly status?: string;
  readonly workspace_id?: string;
  readonly [key: string]: unknown;
}

async function lifecycleAction(verb: string, zombieId: string, env: Env): Promise<unknown> {
  const result = await runZombiectl([verb, zombieId, "--json"], { env });
  if (result.code !== 0) {
    throw new Error(`${verb} ${zombieId} exited ${result.code}: ${result.stderr.trim()}`);
  }
  return result.stdout.trim() ? JSON.parse(result.stdout.trim()) : null;
}

export const stopZombie = (env: Env, id: string): Promise<unknown> => lifecycleAction("stop", id, env);
export const resumeZombie = (env: Env, id: string): Promise<unknown> => lifecycleAction("resume", id, env);
export const killZombie = (env: Env, id: string): Promise<unknown> => lifecycleAction("kill", id, env);

export async function getStatus(env: Env, zombieId: string): Promise<ZombieRow> {
  // `zombiectl status` ignores positional args and lists all zombies in the
  // current workspace (server returns `{items: [...], total}`). Filter
  // client-side. Surface in Discovery: the CLI lacks a per-zombie GET-by-id
  // command — adding one belongs in a follow-on CLI hygiene PR.
  const result = await runZombiectl(["list", "--json"], { env });
  if (result.code !== 0) {
    throw new Error(`list (for status of ${zombieId}) exited ${result.code}: ${result.stderr.trim()}`);
  }
  const payload = JSON.parse(result.stdout.trim() || "{}") as { items?: unknown };
  const items: ZombieRow[] = Array.isArray(payload.items) ? (payload.items as ZombieRow[]) : [];
  const match = items.find((z) => z.id === zombieId || z.zombie_id === zombieId);
  if (!match) {
    throw new Error(`zombie ${zombieId} not found in workspace list: ${result.stdout.slice(0, 400)}`);
  }
  return match;
}

export async function expectStatus(
  env: Env,
  zombieId: string,
  expected: string | ReadonlyArray<string>,
): Promise<ZombieRow> {
  const payload = await getStatus(env, zombieId);
  const actual = payload.status;
  const allowed: ReadonlyArray<string> = Array.isArray(expected) ? expected : [expected as string];
  if (actual === undefined || !allowed.includes(actual)) {
    throw new Error(`expected status ${allowed.join("|")}, got ${actual} for ${zombieId}`);
  }
  return payload;
}
