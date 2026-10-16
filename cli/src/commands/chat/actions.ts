// A shown review is the only setup snapshot an explicit Save or Apply can use.

import { Effect } from "effect";
import { ValidationError, type CliError } from "../../errors/index.ts";
import { OUTPUT_FORMAT, type Output } from "../../services/output.ts";
import { follow, readHistory } from "./follow.ts";
import { stopOnInterrupt } from "./interrupt.ts";
import { isPartial, renderApplied, renderFleet, renderMessage, renderReview, renderResult, schedulesOwed } from "./render.ts";
import { DraftSession, type Sent } from "./session.ts";
import { SetupSession, START, type Start, type PublicationTarget } from "./setup-session.ts";
import { MESSAGE_STATE, type Draft, type Review } from "./protocol.ts";
import { renderSetupRecovery } from "./setup-recovery.ts";

const OPERATION = {send: "send", create: "create", save: "save", publish: "publish", apply: "apply"} as const;
const CHAT_CREATED = "chat_created";
type Operation = typeof OPERATION[keyof typeof OPERATION];
type Wait = Parameters<typeof follow>[3];
type SeenReview = {readonly draft: Draft; readonly review: Review; readonly setupTag?: string};

export class ChatActions {
  readonly #session: DraftSession;
  readonly #setup: SetupSession;
  readonly #output: Output;
  readonly #wait: Wait;
  #seen: SeenReview | undefined;
  #operation: Operation = OPERATION.send;
  #partial = false;

  constructor(session: DraftSession, setup: SetupSession, output: Output, wait: Wait) {
    this.#session = session; this.#setup = setup; this.#output = output; this.#wait = wait;
  }

  get partial(): boolean { return this.#partial; }
  get id(): string | undefined { return this.#session.id; }
  get pending(): boolean { return this.#session.pending || this.#setup.pending; }

  reportRecovery() {
    if (this.#setup.recovery) return renderSetupRecovery(this.#output, this.#setup.recovery);
    const key = this.#session.operationId;
    if (!key) return Effect.void;
    const recovery = {kind: "chat_operation_uncertain", operation_id: key, draft_id: this.#session.id};
    return this.#output.format === OUTPUT_FORMAT.json ? this.#output.printJsonErr(recovery)
      : this.#output.warn(`Chat outcome is unknown. Repeat the same message, source and model with --operation-id ${key}${this.#session.id ? ` on draft ${this.#session.id}` : ""}.`);
  }

  clear(): void { this.#seen = undefined; this.#session.clear(); this.#setup.clear(); }

  history() { return readHistory(this.#session, (message) => renderMessage(this.#output, message)); }

  message(text: string) {
    const self = this;
    return Effect.gen(function* () {
      yield* self.#idleSetup();
      self.#seen = undefined;
      self.#operation = OPERATION.send;
      const sent = yield* self.#session.send(text);
      yield* renderResult(self.#output, `Draft ${sent.draft_id}; accepted message ${sent.sequence}`,
        {kind: "chat_accepted", draft_id: sent.draft_id, sequence: sent.sequence});
      return yield* self.#follow(sent.sequence);
    });
  }

  /** Opens a draft from a source that needs no first message: a Definition, Library revision or fleet setup. */
  create() {
    const self = this;
    return Effect.gen(function* () {
      yield* self.#idleSetup();
      self.#seen = undefined;
      self.#operation = OPERATION.create;
      yield* self.#created(yield* self.#session.create());
    });
  }

  review(name?: string) {
    const self = this;
    return Effect.gen(function* () {
      yield* self.#idleSetup();
      if (name !== undefined) yield* self.#session.patch({instance_name: name});
      const review = yield* self.#session.review();
      const draft = self.#session.draft;
      if (!draft) return yield* missingReview();
      if (draft.linked_fleet_id && review.effective.target_setup_revision) {
        const fleet = yield* self.#setup.fleet(draft.linked_fleet_id);
        if (fleet.setup_revision !== review.effective.target_setup_revision) return yield* missingReview();
        self.#seen = {draft, review, setupTag: fleet.setup_etag};
      } else self.#seen = {draft, review};
      yield* renderReview(self.#output, review);
      return review;
    });
  }

  save(start: Start = {kind: START.none}) {
    const self = this;
    return Effect.gen(function* () {
      yield* self.#idleSetup();
      if (!self.#seen) return yield* missingReview();
      self.#operation = OPERATION.save;
      const fleet = yield* self.#setup.save(self.#seen.draft, self.#seen.review, start);
      self.#partial = isPartial(fleet);
      yield* renderFleet(self.#output, fleet);
      if (self.#partial) yield* self.reportRecovery();
    });
  }

  publish(label: string, target?: PublicationTarget) {
    const self = this;
    return Effect.gen(function* () {
      yield* self.#idleSetup();
      if (!self.#seen) return yield* missingReview();
      self.#operation = OPERATION.publish;
      const published = yield* self.#setup.publish(self.#seen.draft, label, target);
      yield* renderResult(self.#output, `Published Library revision ${published.revision_id}`, published);
    });
  }

  apply() {
    const self = this;
    return Effect.gen(function* () {
      yield* self.#idleSetup();
      if (!self.#seen?.setupTag) return yield* missingReview();
      self.#operation = OPERATION.apply;
      const result = yield* self.#setup.apply(self.#seen.draft, self.#seen.review, self.#seen.setupTag);
      self.#partial = schedulesOwed(result.schedules);
      yield* renderApplied(self.#output, result);
      self.#seen = undefined;
    });
  }

  retry(): Effect.Effect<void, CliError> {
    const self = this;
    return Effect.gen(function* () {
      switch (self.#operation) {
        case OPERATION.send: {
          const sent = yield* self.#session.retry();
          yield* self.#follow(sent.sequence);
          break;
        }
        case OPERATION.create:
          yield* self.#created(yield* self.#session.retry());
          break;
        case OPERATION.save: {
          const fleet = yield* self.#setup.progress();
          self.#partial = isPartial(fleet);
          yield* renderFleet(self.#output, fleet);
          break;
        }
        case OPERATION.publish: {
          const publication = yield* self.#setup.retryPublication();
          yield* renderResult(self.#output, `Published Library revision ${publication.revision_id}`, publication);
          break;
        }
        case OPERATION.apply: {
          const result = yield* self.#setup.retryApply();
          self.#partial = schedulesOwed(result.schedules);
          yield* renderApplied(self.#output, result);
          self.#seen = undefined;
        }
      }
    });
  }

  model(ref: string) {
    return this.#idleSetup().pipe(Effect.andThen(() => this.#session.patch({authoring_model_ref: ref})),
      Effect.tap(() => Effect.sync(() => { this.#seen = undefined; })), Effect.asVoid);
  }

  stop() { return this.#session.stop(); }
  remove() { return this.#session.remove().pipe(Effect.tap(() => Effect.sync(() => this.clear()))); }

  #created(sent: Sent) {
    return renderResult(this.#output, `Draft ${sent.draft_id} created`, {kind: CHAT_CREATED, draft_id: sent.draft_id});
  }

  #follow(sequence: string) {
    return stopOnInterrupt(follow(this.#session, sequence, (message) => renderMessage(this.#output, message), this.#wait),
      this.#session.stop(), this.#output).pipe(
      Effect.tap((message) => Effect.sync(() => { this.#partial = message.state !== MESSAGE_STATE.completed; })),
      Effect.asVoid,
    );
  }

  #idleSetup(): Effect.Effect<void, ValidationError> {
    return this.#setup.pending || this.#session.pending ? Effect.fail(new ValidationError({
      detail: "A chat operation has an unknown outcome", suggestion: "use /retry before changing this draft",
    })) : Effect.void;
  }
}

function missingReview(): Effect.Effect<never, ValidationError> {
  return Effect.fail(new ValidationError({detail: "Inspect the current setup before this action",
    suggestion: "use /review [instance-name], inspect the displayed setup and access, then repeat the explicit action"}));
}
