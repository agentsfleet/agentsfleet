// A private conversation and its uncertain send live only for this invocation.

import { Effect } from "effect";
import { HTTP_METHOD } from "../../constants/http-method.ts";
import { CLI_ERROR_TAG, ValidationError, type NetworkError, type ServerError } from "../../errors/index.ts";
import { DraftSchema, CreatedSchema, AcceptedSchema, MessagesSchema, type Draft, type MessagePage, type Review } from "./protocol.ts";
import { IF_MATCH, type Transport } from "./transport.ts";
import { RetainedRequest } from "./retained-request.ts";

export const DRAFT_SOURCE = {blank: "blank", fleet: "fleet", library: "library", definition: "definition"} as const;
/** An operator-authored native Definition, forwarded whole for the daemon to validate. */
export type DefinitionDocument = Readonly<Record<string, unknown>>;
export type DraftSource =
  | { readonly kind: typeof DRAFT_SOURCE.blank }
  | { readonly kind: typeof DRAFT_SOURCE.fleet; readonly fleet_id: string; readonly setup_revision: string }
  | { readonly kind: typeof DRAFT_SOURCE.library; readonly library_id: string; readonly revision_id: string }
  | { readonly kind: typeof DRAFT_SOURCE.definition; readonly definition: DefinitionDocument };
export type Sent = { readonly draft_id: string; readonly sequence: string };
type ReadError = ServerError | NetworkError | ValidationError;

export class DraftSession {
  readonly #transport: Transport;
  readonly #model: string;
  readonly #source: DraftSource;
  readonly #send: RetainedRequest<Sent>;
  #id: string | undefined;
  #draft: Draft | undefined;
  #sequence = "0";

  constructor(transport: Transport, model: string, source: DraftSource, id?: string, operationId?: string) {
    this.#transport = transport;
    this.#model = model;
    this.#source = source;
    this.#id = id;
    this.#send = new RetainedRequest(operationId);
  }

  get id(): string | undefined { return this.#id; }
  get draft(): Draft | undefined { return this.#draft; }
  get operationId(): string | undefined { return this.#send.operationId; }
  get pending(): boolean { return this.#send.pending; }
  get path(): string { return `${this.#transport.drafts}/${this.#id}`; }

  clear(): void {
    this.#send.clear();
    this.#id = undefined;
    this.#draft = undefined;
    this.#sequence = "0";
  }

  refresh(): Effect.Effect<Draft, ReadError> {
    return this.#guard(Effect.suspend<Draft, ReadError, never>(() => this.#id
      ? this.#transport.call({path: this.path}, DraftSchema).pipe(Effect.tap((draft) => this.#remember(draft)))
      : absent()));
  }

  send(message: string): Effect.Effect<Sent, ReadError> {
    return this.#guard(this.#send.run({message}, (key) => {
      const id = this.#id;
      return id ? this.#transport.call({path: `${this.path}/messages`, method: HTTP_METHOD.post,
        body: {message, operation_id: key}, retry: null}, AcceptedSchema).pipe(
        Effect.map((accepted) => ({draft_id: id, sequence: accepted.sequence})),
        Effect.tap((sent) => Effect.sync(() => { this.#sequence = sent.sequence; })),
      )
      : this.#transport.call({path: this.#transport.drafts, method: HTTP_METHOD.post, retry: null,
        body: {source: this.#source, authoring_model_ref: this.#model, message, operation_id: key}}, CreatedSchema).pipe(
        Effect.tap((draft) => this.#remember(draft)),
        Effect.map((draft) => ({draft_id: draft.id, sequence: draft.accepted.sequence})),
        Effect.tap((sent) => Effect.sync(() => { this.#sequence = sent.sequence; })),
      );
    }));
  }

  /**
   * Opens the draft from its source alone, before any message. A Definition,
   * a pinned Library revision or a fleet's setup revision already carries a
   * complete setup, so it can be reviewed at once. Shares the send's retained
   * identity: either one is the draft's first write.
   */
  create(): Effect.Effect<Sent, ReadError> {
    return this.#guard(this.#send.run({source: this.#source.kind}, (key) => this.#transport.call({
      path: this.#transport.drafts, method: HTTP_METHOD.post, retry: null,
      body: {source: this.#source, authoring_model_ref: this.#model, operation_id: key}}, DraftSchema).pipe(
      Effect.tap((draft) => this.#remember(draft)),
      Effect.map((draft) => ({draft_id: draft.id, sequence: this.#sequence})),
    )));
  }

  retry(): Effect.Effect<Sent, ReadError> { return this.#guard(this.#send.retry()); }

  patch(patch: {authoring_model_ref?: string; instance_name?: string}): Effect.Effect<Draft, ReadError> {
    return this.#guard(this.refresh().pipe(Effect.flatMap((draft) => this.#transport.call({
      path: this.path, method: HTTP_METHOD.patch, headers: {[IF_MATCH]: draft.etag}, body: patch, retry: null,
    }, DraftSchema)), Effect.tap((draft) => this.#remember(draft))));
  }

  review(): Effect.Effect<Review, ReadError> {
    return this.refresh().pipe(Effect.flatMap((draft) => draft.review
      ? Effect.succeed(draft.review)
      : Effect.fail(new ValidationError({detail: "The draft has no validated candidate to review",
        suggestion: "continue chatting until a setup proposal is accepted"}))));
  }

  page(cursor?: string) {
    const query = new URLSearchParams({limit: "25"});
    if (cursor) query.set("starting_after", cursor);
    return this.#guard(Effect.suspend<MessagePage, ReadError, never>(() => this.#id
      ? this.#transport.call({path: `${this.path}/messages?${query}`}, MessagesSchema).pipe(
        Effect.tap((page) => Effect.sync(() => {
          for (const message of page.items) {
            if (BigInt(message.sequence) > BigInt(this.#sequence)) this.#sequence = message.sequence;
          }
        })))
      : absent()));
  }

  stop(): Effect.Effect<void, ReadError> {
    return this.#guard(Effect.suspend<void, ReadError, never>(() => this.#id
      ? this.#transport.empty({path: `${this.path}/cancellations`, method: HTTP_METHOD.post,
        body: {through_sequence: this.#sequence}, retry: null})
      : absent()));
  }

  remove(): Effect.Effect<void, ReadError> {
    return this.#guard(Effect.suspend<void, ReadError, never>(() => this.#id
      ? this.#transport.empty({path: this.path, method: HTTP_METHOD.delete, retry: null})
        .pipe(Effect.tap(() => Effect.sync(() => this.clear())))
      : absent()));
  }

  #remember(draft: Draft) {
    return Effect.sync(() => { this.#id = draft.id; this.#draft = draft; });
  }

  #guard<T, E extends ReadError>(effect: Effect.Effect<T, E>): Effect.Effect<T, E> {
    return effect.pipe(Effect.tapError((error) => Effect.sync(() => {
      if (error._tag === CLI_ERROR_TAG.server && [401, 403, 404, 410].includes(error.status)) this.clear();
    })));
  }
}

function absent(): Effect.Effect<never, ValidationError> {
  return Effect.fail(new ValidationError({
    detail: "This conversation has no accessible draft", suggestion: "send your first message or resume an accessible draft",
  }));
}
