import { Option } from "effect";
import { Command } from "effect/cli";
import { platformModelDelete, platformModelShow, platformModelsList, platformModelWrite } from "../../commands/platform/models.ts";
import { guardedHandler } from "./guarded-handler.ts";
import { platformFileFlag, platformPagingFlags } from "./platform-flags.ts";
import { idArgument } from "./flags.ts";

const opt = Option.getOrUndefined;
const modelId = {id: idArgument("model_id", "Platform catalogue row identifier")};
const list = Command.make("list", platformPagingFlags).pipe(Command.withDescription("List the full admin catalogue (model:read)"),
  guardedHandler(({limit, startingAfter}) => platformModelsList({limit: opt(limit), startingAfter: opt(startingAfter)})));
const show = Command.make("show", modelId).pipe(Command.withDescription("Read one model through the catalogue API (model:read)"),
  guardedHandler(({id}) => platformModelShow(id)));
const create = Command.make("create", {file: platformFileFlag}).pipe(Command.withDescription("Create a priced model from JSON (model:admin)"),
  guardedHandler(({file}) => platformModelWrite(file)));
const update = Command.make("update", {...modelId, file: platformFileFlag}).pipe(Command.withDescription("Replace model settings and price schedule (model:admin)"),
  guardedHandler(({id, file}) => platformModelWrite(file, id)));
const remove = Command.make("delete", modelId).pipe(Command.withDescription("Delete a catalogue model (model:admin)"),
  guardedHandler(({id}) => platformModelDelete(id)));

export const platformModelsCommand = Command.make("models").pipe(
  Command.withDescription("Manage the shared model catalogue"), Command.withSubcommands([list, show, create, update, remove]),
);
