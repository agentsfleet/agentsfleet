// Start a fleet the way a person does from a terminal: `fleet create` with its
// first message. A fleet knows its job only from the conversation, so a probe's
// first message is its whole job description.

import { Schema } from "effect";
import { runFleetctl } from "./cli.js";

type Env = Readonly<Record<string, string>>;

const COMMAND_TIMEOUT_MS = 120_000;
/** What a probe does with this message and every later one. */
export const PROBE_MESSAGE =
  "Reply to this and every later message with ACK followed by the message text. Do not call tools." as const;

const CreatedSchema = Schema.Struct({ fleet_id: Schema.String, turn_id: Schema.String });
export type CreatedFleet = typeof CreatedSchema.Type;

export interface CreateFleetOptions {
  readonly name: string;
  readonly message?: string;
  readonly timeoutMs?: number;
}

/** `fleet create --json`, refusing anything but exit 0 and `{fleet_id, turn_id}`. */
export async function createFleetFromCli(env: Env, options: CreateFleetOptions): Promise<CreatedFleet> {
  const result = await runFleetctl(
    ["--json", "fleet", "create", options.message ?? PROBE_MESSAGE, "--name", options.name],
    { env, timeoutMs: options.timeoutMs ?? COMMAND_TIMEOUT_MS },
  );
  if (result.code !== 0) throw new Error(`fleet create exited ${result.code}: ${result.stderr.trim()}`);
  return Schema.decodeUnknownSync(CreatedSchema)(JSON.parse(result.stdout.trim()));
}
