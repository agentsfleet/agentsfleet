// The terminal uses the same authorized draft and setup resources as the app.

import { Effect } from "effect";
import { ValidationError } from "../../errors/index.ts";
import { isRecord } from "../../lib/guards.ts";
import { CliConfig } from "../../services/config.ts";
import { HttpClient } from "../../services/http-client.ts";
import { Input } from "../../services/input.ts";
import { Output, OUTPUT_FORMAT } from "../../services/output.ts";
import { Stdin } from "../../services/stdin.ts";
import { requireWorkspaceId, resolveAuthToken, requireValidId } from "../workspace-guards.ts";
import { ChatActions } from "./actions.ts";
import { waitNotice } from "./notifications.ts";
import { runChatRepl } from "./repl.ts";
import { DraftSession, DRAFT_SOURCE, type DefinitionDocument, type DraftSource } from "./session.ts";
import { SetupSession, START } from "./setup-session.ts";
import { createTransport } from "./transport.ts";
import { recoverSetup, recoveryKey } from "./command-recovery.ts";

export interface ChatOptions {
  readonly operationId?: string | undefined;
  readonly candidateId?: string | undefined;
  readonly setupEtag?: string | undefined;
  readonly publishTo?: string | undefined;
  readonly libraryEtag?: string | undefined;
  readonly draftId?: string | undefined;
  readonly model?: string | undefined;
  readonly message?: string | undefined;
  readonly fleet?: string | undefined;
  readonly setupRevision?: string | undefined;
  readonly library?: string | undefined;
  readonly revision?: string | undefined;
  readonly definition?: string | undefined;
  readonly noInput: boolean;
  readonly review: boolean;
  readonly save: boolean;
  readonly apply: boolean;
  readonly stop: boolean;
  readonly name?: string | undefined;
  readonly reviewDigest?: string | undefined;
  readonly runNow?: string | undefined;
  readonly publish?: string | undefined;
}

const USAGE = "agentsfleet chat [draft-id] --model <authoring-ref>";
const DECIMAL_REVISION = /^[1-9][0-9]*$/u;
const BYTES_PER_KIB = 1024;
/** `MAX_DEFINITION_BYTES` in rustd/crates/afd_fleet_runtime/src/definition/mod.rs: longer Definition text is refused there. */
export const MAX_DEFINITION_BYTES = 448 * BYTES_PER_KIB;
const DEFINITION_HINT = "pass --definition a readable file holding one native Definition JSON object";

export function chatEffect(options: ChatOptions) {
  return Effect.gen(function* () {
    yield* validate(options);
    const definition = options.definition === undefined ? undefined : yield* readDefinition(options.definition);
    const workspace = yield* requireWorkspaceId;
    const token = yield* resolveAuthToken;
    const http = yield* HttpClient;
    const config = yield* CliConfig;
    const input = yield* Input;
    const output = yield* Output;
    const stdin = yield* Stdin;
    const transport = createTransport(http, workspace, token);
    const recovery = recoveryKey(options);
    const setup = new SetupSession(transport, recovery);
    const session = new DraftSession(transport, options.model ?? "", source(options, definition), options.draftId, recovery ? undefined : options.operationId);
    const actions = new ChatActions(session, setup, output, (sequence, state) => waitNotice({
      url: `${config.apiUrl.replace(/\/$/u, "")}${session.path}/events/stream`, token, sequence, state,
      ...(config.fetchImpl ? {fetchImpl: config.fetchImpl} : {}),
    }));
    const interactive = !options.noInput && stdin.isTTY && output.format === OUTPUT_FORMAT.text;
    const operation = recovery ? recoverSetup(setup, options, output) : run(actions, options, interactive, input, output);
    return yield* operation.pipe(Effect.tapError(() => actions.reportRecovery()),
      Effect.ensuring(Effect.sync(() => actions.clear())));
  });
}

function run(actions: ChatActions, options: ChatOptions, interactive: boolean, input: Input, output: Output) {
  return Effect.gen(function* () {
    if (options.draftId) yield* actions.history();
    if (options.stop) { yield* actions.stop(); return 0; }
    if (options.message !== undefined) yield* actions.message(options.message);
    else if (carriesSetup(options) && !options.draftId) yield* actions.create();
    if (options.review || options.save || options.apply || options.publish) {
      const review = yield* actions.review(options.name);
      if (options.save || options.apply || options.publish) {
        if (options.reviewDigest !== review.review_digest) return yield* Effect.fail(new ValidationError({
          detail: "Supply the digest of the setup you inspected", suggestion: "run with --review, inspect its output, then use --review-digest <digest>",
        }));
      }
      if (options.save) yield* actions.save(options.runNow === undefined ? {kind: START.none} : {kind: START.runNow, message: options.runNow});
      if (options.apply) yield* actions.apply();
      if (options.publish) yield* actions.publish(options.publish,
        options.publishTo && options.libraryEtag ? {id: options.publishTo, etag: options.libraryEtag} : undefined);
    }
    if (interactive) return yield* runChatRepl(actions, input, output);
    if (!options.draftId && options.message === undefined && !carriesSetup(options)) return yield* fail("A message is required outside an interactive terminal");
    return actions.partial ? 1 : 0;
  });
}

function validate(options: ChatOptions) {
  return Effect.gen(function* () {
    const setupRecovery = Boolean(options.operationId && options.candidateId);
    if (options.operationId) {
      yield* requireValidId(options.operationId, "operation_id", USAGE);
      if (!options.noInput || options.stop || options.review) return yield* fail("--operation-id recovers one --no-input message, draft creation or explicit setup action");
      if (setupRecovery ? options.message !== undefined || !(options.save || options.apply || options.publish)
        : (options.message === undefined && !carriesSetup(options)) || options.save || options.apply || options.publish) return yield* fail("Choose one original message, draft creation or setup action to recover");
    }
    if (options.candidateId) yield* requireValidId(options.candidateId, "draft_candidate_id", USAGE);
    if (options.candidateId && !options.operationId || options.setupEtag && !(setupRecovery && options.apply)) return yield* fail("Recovery identity fields require their matching --operation-id setup action");
    if (Boolean(options.publishTo) !== Boolean(options.libraryEtag) || options.publishTo && !options.publish) return yield* fail("Owned revision publication requires --publish, --publish-to and the inspected --library-etag");
    if (options.draftId) yield* requireValidId(options.draftId, "draft_id", USAGE);
    if (!setupRecovery && !options.draftId && !options.model) return yield* fail("Choose an authoring model with --model");
    if (options.draftId && (options.fleet && !setupRecovery || options.library || options.definition !== undefined || options.model)) return yield* fail("A resumed draft keeps its source and model; use /model for future messages");
    if (Number(Boolean(options.fleet)) + Number(Boolean(options.library)) + Number(options.definition !== undefined) > 1) return yield* fail("Choose one source: --fleet, --library or --definition");
    if (options.fleet) yield* requireValidId(options.fleet, "fleet_id", USAGE);
    if (options.fleet && !setupRecovery && !DECIMAL_REVISION.test(options.setupRevision ?? "")) return yield* fail("--fleet requires --setup-revision <positive-decimal>");
    if (options.library && !options.revision) return yield* fail("--library requires --revision <revision-id>");
    if (options.revision) yield* requireValidId(options.revision, "revision_id", USAGE);
    if (options.setupRevision && !options.fleet || options.revision && !options.library) return yield* fail("Revision selection requires its matching source");
    if (options.runNow !== undefined && !options.save) return yield* fail("--run-now requires explicit --save");
    if (Number(options.save) + Number(options.apply) + Number(Boolean(options.publish)) > 1) return yield* fail("Choose one explicit setup action per invocation");
  });
}

// A Definition, a pinned Library revision and a fleet's setup revision each
// hold a complete setup, so their draft opens without a first message and can
// be reviewed at once. Only a blank draft needs text before anything exists.
function carriesSetup(options: ChatOptions): boolean {
  return options.definition !== undefined || Boolean(options.library) || Boolean(options.fleet);
}

function source(options: ChatOptions, definition?: DefinitionDocument): DraftSource {
  if (definition) return {kind: DRAFT_SOURCE.definition, definition};
  if (options.fleet && options.setupRevision) return {kind: DRAFT_SOURCE.fleet, fleet_id: options.fleet, setup_revision: options.setupRevision};
  if (options.library && options.revision) return {kind: DRAFT_SOURCE.library, library_id: options.library, revision_id: options.revision};
  return {kind: DRAFT_SOURCE.blank};
}

// The file is read whole, so its size is checked before reading. Everything
// else about the Definition is the daemon's to validate and name.
function readDefinition(path: string): Effect.Effect<DefinitionDocument, ValidationError> {
  return Effect.gen(function* () {
    const file = Bun.file(path);
    if (file.size > MAX_DEFINITION_BYTES) return yield* Effect.fail(definitionError(path, `is ${file.size} bytes; a Definition is at most ${MAX_DEFINITION_BYTES}`));
    const text = yield* Effect.tryPromise({try: () => file.text(), catch: (cause) => definitionError(path, `cannot be read: ${reason(cause)}`)});
    const parsed = yield* Effect.try({try: (): unknown => JSON.parse(text), catch: (cause) => definitionError(path, `is not valid JSON: ${reason(cause)}`)});
    if (!isRecord(parsed) || Array.isArray(parsed)) return yield* Effect.fail(definitionError(path, "must hold one JSON object"));
    return parsed;
  });
}

function definitionError(path: string, problem: string): ValidationError {
  return new ValidationError({detail: `Definition file ${path} ${problem}`, suggestion: DEFINITION_HINT});
}

function reason(cause: unknown): string {
  return cause instanceof Error ? cause.message : String(cause);
}

function fail(detail: string): Effect.Effect<never, ValidationError> {
  return Effect.fail(new ValidationError({detail, suggestion: USAGE}));
}
