import { Command } from "effect/cli";
import { platformModelDelete, platformModelShow, platformModelsList, platformModelWrite } from "../../commands/platform/models.ts";
import { guardedHandler } from "./guarded-handler.ts";
import { grants, listedWhen, type Visibility } from "./visibility.ts";
import { PLATFORM_SCOPE, platformFileFlag, platformPagingFlags } from "./platform-flags.ts";
import { idArgument, opt } from "./flags.ts";

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

export const platformModelsCommand = (visibility: Visibility) => {
  const read = grants(visibility, PLATFORM_SCOPE.modelRead);
  const admin = grants(visibility, PLATFORM_SCOPE.modelAdmin);
  return {
    listed: read || admin,
    command: Command.make("models").pipe(
      Command.withDescription("Manage the shared model catalogue"),
      Command.withSubcommands([
        listedWhen(read)(list), listedWhen(read)(show),
        listedWhen(admin)(create), listedWhen(admin)(update), listedWhen(admin)(remove),
      ]),
    ),
  };
};
