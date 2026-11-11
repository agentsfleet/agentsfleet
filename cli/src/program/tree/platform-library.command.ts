import { Argument, Command } from "effect/cli";
import { platformLibraryDelete, platformLibraryList, platformLibraryPublish, platformLibraryStage, platformLibraryUpdate } from "../../commands/platform/library.ts";
import { guardedHandler } from "./guarded-handler.ts";
import { platformEtagFlag, platformFileFlag } from "./platform-flags.ts";
import { requiredIdFlag, requiredTextFlag } from "./flags.ts";

const entryId = {id: Argument.String("library_id").pipe(Argument.withDescription("Stable platform library identifier"))};
const conditional = {...entryId, etag: platformEtagFlag};
const list = Command.make("list").pipe(Command.withDescription("List staged and published entries (platform-library:write)"), guardedHandler(() => platformLibraryList()));
const show = Command.make("show", entryId).pipe(Command.withDescription("Inspect metadata and the current version tag (platform-library:write)"),
  guardedHandler(({id}) => platformLibraryList(id)));
const stage = Command.make("stage", {
  workspace: requiredIdFlag("workspace", "Workspace containing your private candidate"),
  candidate: requiredIdFlag("candidate-id", "Creator-owned candidate returned by chat review"),
  label: requiredTextFlag("label", "Library display label"),
  operation: requiredIdFlag("operation-id", "Replay identity; reuse this value if the result is uncertain"),
}).pipe(Command.withDescription("Stage a reviewed candidate without publishing it (platform-library:write)"),
  guardedHandler(({workspace, candidate, label, operation}) => platformLibraryStage(workspace, candidate, label, operation)));
const update = Command.make("update", {...conditional, file: platformFileFlag}).pipe(Command.withDescription("Edit name, description, and credential reasons (platform-library:write)"),
  guardedHandler(({id, file, etag}) => platformLibraryUpdate(id, file, etag)));
const publication = (name: "publish" | "unpublish", published: boolean) => Command.make(name, conditional).pipe(
  Command.withDescription(`${name} the inspected entry (platform-library:write)`), guardedHandler(({id, etag}) => platformLibraryPublish(id, published, etag)),
);
const remove = Command.make("delete", conditional).pipe(Command.withDescription("Delete the inspected platform entry (platform-library:write)"),
  guardedHandler(({id, etag}) => platformLibraryDelete(id, etag)));

export const platformLibraryCommand = Command.make("fleet-library").pipe(
  Command.withDescription("Manage the platform fleet library"),
  Command.withSubcommands([list, show, stage, update, publication("publish", true), publication("unpublish", false), remove]),
);
