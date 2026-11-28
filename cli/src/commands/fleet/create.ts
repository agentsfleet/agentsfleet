// `agentsfleet fleet create "<message>"` — a fleet starts from its first message.
//
// One POST writes the fleet and the message's turn together; nothing is set
// up first. The name and the model are optional: the daemon names the fleet
// from its id and runs it on the workspace's selected model unless told
// otherwise.

import type { Redacted } from "effect";
import type { CliConfig } from "../../services/config.ts";
import type { Credentials } from "../../services/credentials.ts";
import type { Workspaces } from "../../services/workspaces.ts";
import { Effect, Schema } from "effect";
import { v7 as uuidv7 } from "uuid";
import { HTTP_METHOD } from "../../constants/http-method.ts";
import { ServerError, ValidationError, type CliError } from "../../errors/index.ts";
import { wsFleetsPath } from "../../lib/api-paths.ts";
import { HttpClient } from "../../services/http-client.ts";
import { Output } from "../../services/output.ts";
import { requireWorkspaceId, resolveAuthToken } from "../workspace/guards.ts";
import { writesJson } from "./read.ts";
import { DISPOSITION_QUEUED, DISPOSITION_STARTED } from "./steer-placement.ts";
import { messageImage, type MessageImage } from "./message-image.ts";

export interface FleetCreateInput {
  readonly message: string;
  readonly name?: string | undefined;
  readonly model?: string | undefined;
  readonly image?: string | undefined;
}

export const FLEET_CREATE_USAGE = 'usage: agentsfleet fleet create "<message>" [--name <name>] [--model <ref>] [--image <path>]' as const;
const CREATE_RESPONSE_INVALID = "FLEET_CREATE_RESPONSE_INVALID";
const BAD_GATEWAY = 502;

// `201 {fleet, accepted: {turn_id, disposition}, replayed}`, as far as a caller reads it.
const CreatedSchema = Schema.Struct({
  fleet: Schema.Struct({ id: Schema.String, name: Schema.String }),
  accepted: Schema.Struct({
    turn_id: Schema.String,
    disposition: Schema.Literals([DISPOSITION_STARTED, DISPOSITION_QUEUED]),
  }),
});
type CreatedFleet = typeof CreatedSchema.Type;

/**
 * Writes the fleet and its first turn. The operation id is minted per call: it
 * lets the daemon answer a transport retry with the fleet it already wrote.
 */
const createFleet = (
  wsId: string,
  token: Redacted.Redacted<string>,
  input: FleetCreateInput,
  image: MessageImage | undefined,
): Effect.Effect<CreatedFleet, CliError, HttpClient> =>
  Effect.gen(function* () {
    const http = yield* HttpClient;
    const body = yield* http.request<unknown>({
      path: wsFleetsPath(wsId),
      method: HTTP_METHOD.post,
      token,
      body: {
        message: input.message,
        operation_id: uuidv7(),
        ...(input.model === undefined ? {} : { model_ref: input.model }),
        ...(input.name === undefined ? {} : { name: input.name }),
        ...(image === undefined ? {} : { images: [image] }),
      },
    });
    return yield* Schema.decodeUnknownEffect(CreatedSchema)(body).pipe(Effect.mapError(() => new ServerError({
      detail: "The fleet was created, but its answer did not match the supported API shape",
      suggestion: "run `agentsfleet fleet list` to find it; update the CLI to match the daemon build",
      code: CREATE_RESPONSE_INVALID, status: BAD_GATEWAY, requestId: null,
    })));
  });

export const fleetCreateEffect = (
  message: string | undefined,
  flags: Omit<FleetCreateInput, "message">,
): Effect.Effect<void, CliError, CliConfig | Credentials | HttpClient | Output | Workspaces> =>
  Effect.gen(function* () {
    if (message === undefined || message.trim().length === 0) {
      return yield* Effect.fail(new ValidationError({ detail: "message is required", suggestion: FLEET_CREATE_USAGE }));
    }
    const image = flags.image === undefined ? undefined : yield* messageImage(flags.image);
    const wsId = yield* requireWorkspaceId;
    const token = yield* resolveAuthToken;
    const created = yield* createFleet(wsId, token, { ...flags, message }, image);
    const output = yield* Output;
    const { id, name } = created.fleet;
    const { turn_id: turnId, disposition } = created.accepted;
    if (writesJson(output)) {
      return yield* output.printJson({ fleet_id: id, turn_id: turnId });
    }
    yield* output.success(`Started fleet ${name} (${id}). First turn ${turnId}: ${disposition}.`);
    yield* output.info(`Talk to it with: agentsfleet fleet steer ${id}`);
  });
