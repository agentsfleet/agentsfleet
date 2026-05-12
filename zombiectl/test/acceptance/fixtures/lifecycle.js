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
  const result = await runZombiectl(["status", zombieId, "--json"], { env });
  if (result.code !== 0) {
    throw new Error(`status ${zombieId} exited ${result.code}: ${result.stderr.trim()}`);
  }
  return JSON.parse(result.stdout.trim());
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
