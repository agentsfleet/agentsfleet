// Ordinary terminal chat, backed by creator-private workspace drafts.

import { Option } from "effect";
import { Argument, Command, Flag } from "effect/cli";
import { chatEffect } from "../../commands/chat/commands.ts";
import { withManagedExitCode } from "../../runtime/exit-code.service.ts";
import { METAVAR } from "./flags.ts";
import { guardedHandler } from "./guarded-handler.ts";
import { chatHistoryCommands } from "./chat-history.command.ts";

const opt = Option.getOrUndefined;
const textFlag = (name: string, description: string, metavar: string) =>
  Flag.String(name).pipe(Flag.withDescription(description), Flag.withMetavar(metavar), Flag.optional);
const choiceFlag = (name: string, description: string) => Flag.Boolean(name).pipe(Flag.withDescription(description), Flag.withDefault(false));

export const chatCommand = Command.make("chat", {
  draftId: Argument.String("draft-id").pipe(Argument.optional),
  operationId: textFlag("operation-id", "Retry an unconfirmed --no-input message, draft creation or setup action with its original identity", METAVAR.id),
  candidateId: textFlag("candidate-id", "Original candidate identifier from setup recovery metadata", METAVAR.id),
  setupEtag: textFlag("setup-etag", "Original inspected fleet setup tag for uncertain Apply recovery", METAVAR.etag),
  publishTo: textFlag("publish-to", "Append a revision to this owned Library entry", METAVAR.id),
  libraryEtag: textFlag("library-etag", "Inspected Library tag required by --publish-to", METAVAR.etag),
  model: textFlag("model", "Authoring model reference for a new private draft", METAVAR.ref),
  message: textFlag("message", "Send ordinary chat text and follow its durable outcome", METAVAR.text),
  fleet: textFlag("fleet", "Improve this existing fleet; no first message is needed", METAVAR.id),
  setupRevision: textFlag("setup-revision", "Exact fleet setup revision to improve", METAVAR.count),
  library: textFlag("library", "Start from a reusable Library entry; no first message is needed", METAVAR.id),
  revision: textFlag("revision", "Exact immutable Library revision", METAVAR.id),
  definition: textFlag("definition", "Start from this native Definition JSON file; no first message is needed", METAVAR.path),
  noInput: choiceFlag("no-input", "Run only the supplied actions without a terminal prompt"),
  review: choiceFlag("review", "Inspect the full current setup, access, readiness, and review digest"),
  save: choiceFlag("save", "Explicitly save the previously reviewed setup"),
  apply: choiceFlag("apply", "Explicitly apply the reviewed improvement to its source fleet"),
  stop: choiceFlag("stop", "Cancel through the latest accepted message in this draft"),
  name: textFlag("name", "Select the instance name before reviewing", METAVAR.name),
  reviewDigest: textFlag("review-digest", "Digest from the setup you inspected", METAVAR.digest),
  runNow: textFlag("run-now", "With --save, admit this initial runtime message once", METAVAR.text),
  publish: textFlag("publish", "Explicitly publish a reusable Library entry with this label", METAVAR.label),
}).pipe(
  Command.withDescription("Create, resume, and review a private workspace chat"),
  guardedHandler((flags) => withManagedExitCode(chatEffect({
    operationId: opt(flags.operationId), draftId: opt(flags.draftId), model: opt(flags.model), message: opt(flags.message),
    candidateId: opt(flags.candidateId), setupEtag: opt(flags.setupEtag), publishTo: opt(flags.publishTo), libraryEtag: opt(flags.libraryEtag),
    fleet: opt(flags.fleet), setupRevision: opt(flags.setupRevision), library: opt(flags.library), revision: opt(flags.revision),
    definition: opt(flags.definition),
    noInput: flags.noInput, review: flags.review, save: flags.save, apply: flags.apply, stop: flags.stop,
    name: opt(flags.name), reviewDigest: opt(flags.reviewDigest), runNow: opt(flags.runNow), publish: opt(flags.publish),
  }))),
  Command.withSubcommands(chatHistoryCommands),
);
