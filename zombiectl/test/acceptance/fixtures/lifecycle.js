/**
 * Shared lifecycle action helpers — stop / resume / kill / expectStatus.
 *
 * Each helper composes a `runZombiectl` call, asserts exit 0, and
 * (for status) returns the parsed JSON envelope.
 */

import { runZombiectl } from "./cli.js";

async function lifecycleAction(verb, zombieId, env) {
  const result = await runZombiectl([verb, zombieId, "--json"], { env });
  if (result.code !== 0) {
    throw new Error(`${verb} ${zombieId} exited ${result.code}: ${result.stderr.trim()}`);
  }
  return result.stdout.trim() ? JSON.parse(result.stdout.trim()) : null;
}

export const stopZombie = (env, id) => lifecycleAction("stop", id, env);
export const resumeZombie = (env, id) => lifecycleAction("resume", id, env);
export const killZombie = (env, id) => lifecycleAction("kill", id, env);

export async function getStatus(env, zombieId) {
  // `zombiectl status` ignores positional args and lists all zombies in the
  // current workspace (server returns `{items: [...], total}`). Filter
  // client-side. Surface in Discovery: the CLI lacks a per-zombie GET-by-id
  // command — adding one belongs in a follow-on CLI hygiene PR.
  const result = await runZombiectl(["list", "--json"], { env });
  if (result.code !== 0) {
    throw new Error(`list (for status of ${zombieId}) exited ${result.code}: ${result.stderr.trim()}`);
  }
  const payload = JSON.parse(result.stdout.trim() || "{}");
  const items = Array.isArray(payload.items) ? payload.items : [];
  const match = items.find((z) => z.id === zombieId || z.zombie_id === zombieId);
  if (!match) {
    throw new Error(`zombie ${zombieId} not found in workspace list: ${result.stdout.slice(0, 400)}`);
  }
  return match;
}

export async function expectStatus(env, zombieId, expected) {
  const payload = await getStatus(env, zombieId);
  const actual = payload.status;
  const allowed = Array.isArray(expected) ? expected : [expected];
  if (!allowed.includes(actual)) {
    throw new Error(`expected status ${allowed.join("|")}, got ${actual} for ${zombieId}`);
  }
  return payload;
}
