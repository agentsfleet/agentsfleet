// A probe fleet started from its first message with `fleet create`, named for
// this run so prefix-scoped teardown finds it. A fleet is active from that
// message, so there is nothing to wait for before steering it.

import crypto from "node:crypto";

import { ACCEPTANCE_RUN_PREFIX } from "./constants.ts";
import { ensurePlatformSecretsSeeded } from "./platform-secrets.ts";
import { createFleetFromCli } from "./fleet-create.ts";

export interface ProbeOptions {
  readonly env: Readonly<Record<string, string>>;
  readonly timeoutMs?: number;
  readonly seedFixtureSecrets?: boolean;
  readonly runPrefix?: string;
}

export interface ProbeFleet {
  readonly id: string;
  readonly name: string;
  /** The first message's turn, already queued or running when this returns. */
  readonly turn_id: string;
}

const DEFAULT_CREATE_TIMEOUT_MS = 120_000;

function uniqueName(runPrefix: string, fixtureName: string): string {
  return `${runPrefix}-${fixtureName}-${crypto.randomBytes(3).toString("hex")}`;
}

export async function createProbeFleet(opts: ProbeOptions): Promise<ProbeFleet> {
  const timeoutMs = opts.timeoutMs ?? DEFAULT_CREATE_TIMEOUT_MS;
  if (opts.seedFixtureSecrets) await ensurePlatformSecretsSeeded(opts.env, () => timeoutMs);
  const name = uniqueName(opts.runPrefix ?? ACCEPTANCE_RUN_PREFIX, "probe");
  const created = await createFleetFromCli(opts.env, { name, timeoutMs });
  return { id: created.fleet_id, name, turn_id: created.turn_id };
}
