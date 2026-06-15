/**
 * Shared lifecycle action helpers — stop / resume / kill / expectStatus.
 *
 * Each helper composes a `runAgentctl` call, asserts exit 0, and
 * (for status) returns the parsed JSON envelope.
 */

import { runAgentctl } from "./cli.js";

type Env = Readonly<Record<string, string>>;

export interface AgentRow {
  readonly id?: string;
  readonly agent_id?: string;
  readonly name?: string;
  readonly status?: string;
  readonly workspace_id?: string;
  readonly [key: string]: unknown;
}

async function lifecycleAction(verb: string, agentId: string, env: Env): Promise<unknown> {
  const result = await runAgentctl([verb, agentId, "--json"], { env });
  if (result.code !== 0) {
    throw new Error(`${verb} ${agentId} exited ${result.code}: ${result.stderr.trim()}`);
  }
  return result.stdout.trim() ? JSON.parse(result.stdout.trim()) : null;
}

export const stopAgent = (env: Env, id: string): Promise<unknown> => lifecycleAction("stop", id, env);
export const resumeAgent = (env: Env, id: string): Promise<unknown> => lifecycleAction("resume", id, env);
export const killAgent = (env: Env, id: string): Promise<unknown> => lifecycleAction("kill", id, env);

export async function getStatus(env: Env, agentId: string): Promise<AgentRow> {
  // `agentsfleet status` ignores positional args and lists all agents in the
  // current workspace (server returns `{items: [...], total}`). Filter
  // client-side. Surface in Discovery: the CLI lacks a per-agent GET-by-id
  // command — adding one belongs in a follow-on CLI hygiene PR.
  const result = await runAgentctl(["list", "--json"], { env });
  if (result.code !== 0) {
    throw new Error(`list (for status of ${agentId}) exited ${result.code}: ${result.stderr.trim()}`);
  }
  const payload = JSON.parse(result.stdout.trim() || "{}") as { items?: unknown };
  const items: AgentRow[] = Array.isArray(payload.items) ? (payload.items as AgentRow[]) : [];
  const match = items.find((z) => z.id === agentId || z.agent_id === agentId);
  if (!match) {
    throw new Error(`agent ${agentId} not found in workspace list: ${result.stdout.slice(0, 400)}`);
  }
  return match;
}

export async function expectStatus(
  env: Env,
  agentId: string,
  expected: string | ReadonlyArray<string>,
): Promise<AgentRow> {
  const payload = await getStatus(env, agentId);
  const actual = payload.status;
  const allowed: ReadonlyArray<string> = Array.isArray(expected) ? expected : [expected as string];
  if (actual === undefined || !allowed.includes(actual)) {
    throw new Error(`expected status ${allowed.join("|")}, got ${actual} for ${agentId}`);
  }
  return payload;
}
