import { Effect, Schema } from "effect";
import { HTTP_METHOD } from "../../constants/http-method.ts";
import { decodePlatform, platformPath, platformRequest, platformResult } from "./common.ts";

const ADMIN_WORKSPACES_PATH = "/v1/admin/workspaces";
const ROTATE_SUFFIX = "/checkpoint-key/rotate";
const rotated = Schema.Struct({retired_generations: Schema.Number, retired_objects: Schema.Number});

/** Writes a new checkpoint key for one workspace and prints what the old key's retirement removed. */
export const platformCheckpointKeyRotate = Effect.fn("platform.checkpointKey.rotate")(function* (workspace: string) {
  const path = platformPath(ADMIN_WORKSPACES_PATH, workspace, ROTATE_SUFFIX);
  const result = yield* decodePlatform(rotated, yield* platformRequest({path, method: HTTP_METHOD.post}));
  yield* platformResult({workspace_id: workspace, ...result}, "Checkpoint key rotated", {
    Workspace: workspace,
    "Checkpoints retired": String(result.retired_generations),
    "Stored objects to delete": String(result.retired_objects),
  });
});
