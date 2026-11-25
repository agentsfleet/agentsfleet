import { Command } from "effect/cli";
import { platformModelDelete, platformModelShow, platformModelsList, platformModelWrite } from "../../commands/platform/models.ts";
import { guardedHandler } from "./guarded-handler.ts";
import { grants, listedWhen, type Visibility } from "./visibility.ts";
import { PLATFORM_SCOPE, platformFileFlag, platformPagingFlags } from "./platform-flags.ts";
import { idArgument, opt } from "./flags.ts";
import { platformModelDefaultCommand } from "./platform-model-default.command.ts";

const modelId = {id: idArgument("model_id", "Model ID from `agentsfleet platform models list`")};
const list = Command.make("list", platformPagingFlags).pipe(Command.withDescription("List every model in the catalogue (model:read)"),
  guardedHandler(({limit, startingAfter}) => platformModelsList({limit: opt(limit), startingAfter: opt(startingAfter)})));
const show = Command.make("show", modelId).pipe(Command.withDescription("Show one model (model:read)"),
  guardedHandler(({id}) => platformModelShow(id)));
const create = Command.make("create", {file: platformFileFlag}).pipe(Command.withDescription("Add a model and its prices from a JSON file (model:admin)"),
  guardedHandler(({file}) => platformModelWrite(file)));
const update = Command.make("update", {...modelId, file: platformFileFlag}).pipe(Command.withDescription("Replace a model's settings and prices (model:admin)"),
  guardedHandler(({id, file}) => platformModelWrite(file, id)));
const remove = Command.make("delete", modelId).pipe(Command.withDescription("Remove a model from the catalogue (model:admin)"),
  guardedHandler(({id}) => platformModelDelete(id)));

export const platformModelsCommand = (visibility: Visibility) => {
  const read = grants(visibility, PLATFORM_SCOPE.modelRead);
  const admin = grants(visibility, PLATFORM_SCOPE.modelAdmin);
  const defaults = platformModelDefaultCommand(visibility);
  return {
    listed: read || admin || defaults.listed,
    command: Command.make("models").pipe(
      Command.withDescription("Manage the models every account can choose from"),
      Command.withSubcommands([
        listedWhen(read)(list), listedWhen(read)(show),
        listedWhen(admin)(create), listedWhen(admin)(update), listedWhen(admin)(remove),
        listedWhen(defaults.listed)(defaults.command),
      ]),
    ),
  };
};
