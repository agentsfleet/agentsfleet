// Explicit setup mutations keep the exact reviewed body and operation identity.

import { Effect } from "effect";
import { HTTP_METHOD } from "../../constants/http-method.ts";
import { ValidationError, type NetworkError, type ServerError } from "../../errors/index.ts";
import { AppliedSchema, FleetSchema, type Applied, type Draft, type Fleet, type Review } from "./protocol.ts";
import { IF_MATCH, IDEMPOTENCY_KEY, type Transport } from "./transport.ts";
import { RetainedRequest } from "./retained-request.ts";
import { SETUP_ACTION, type SetupRecovery, type RecoveryKey } from "./setup-recovery.ts";
import { isPartial } from "./render.ts";

export const START = {none: "none", runNow: "run_now"} as const;
export type Start = {readonly kind: typeof START.none} | {readonly kind: typeof START.runNow; readonly message: string};
type RequestError = NetworkError | ServerError;
type SetupError = RequestError | ValidationError;

export class SetupSession {
  readonly #transport: Transport;
  readonly #save: RetainedRequest<Fleet>;
  readonly #apply: RetainedRequest<Applied>;
  #recovery: SetupRecovery | undefined;
  #continueSave: Effect.Effect<Fleet, RequestError> | undefined;
  #fleet: Fleet | undefined;

  constructor(transport: Transport, recovery?: RecoveryKey) {
    this.#transport = transport;
    this.#save = new RetainedRequest(recovery?.action === SETUP_ACTION.save ? recovery.key : undefined);
    this.#apply = new RetainedRequest(recovery?.action === SETUP_ACTION.apply ? recovery.key : undefined);
  }

  get pending(): boolean { return this.#save.pending || this.#apply.pending; }
  get recovery(): SetupRecovery | undefined {
    return this.pending || (this.#fleet && isPartial(this.#fleet)) ? this.#recovery : undefined;
  }

  clear(): void {
    this.#save.clear(); this.#apply.clear();
    this.#continueSave = undefined; this.#fleet = undefined;
    this.#recovery = undefined;
  }

  save(draft: Draft, review: Review, start: Start): Effect.Effect<Fleet, SetupError> {
    return Effect.suspend<Fleet, SetupError, never>(() => {
      if (this.#apply.pending) return unresolved();
      const candidate = draft.candidate;
      if (!candidate) return missingCandidate();
      return this.saveCandidate(candidate.id, review.effective.instance_name, review.review_digest, start);
    });
  }

  saveCandidate(candidateId: string, name: string, reviewDigest: string, start: Start) {
    return Effect.suspend<Fleet, SetupError, never>(() => {
      if (this.#apply.pending) return unresolved();
      const body = {source: {kind: "draft", draft_candidate_id: candidateId}, name, review_digest: reviewDigest, start, origin: "cli"};
      return this.#save.run(body, (key) => {
        this.#recovery = {action: SETUP_ACTION.save, operation_id: key, draft_candidate_id: candidateId,
          name, review_digest: reviewDigest, start_kind: start.kind};
        const request = this.#transport.call({path: this.#transport.fleets, method: HTTP_METHOD.post,
          headers: {[IDEMPOTENCY_KEY]: key}, body, retry: null}, FleetSchema);
        this.#continueSave = request;
        return request;
      }).pipe(Effect.tap((fleet) => Effect.sync(() => { this.#fleet = fleet; })),
        Effect.tapError(() => Effect.sync(() => { if (!this.#save.pending) this.#recovery = undefined; })));
    });
  }

  progress(): Effect.Effect<Fleet, SetupError> {
    return Effect.suspend<Fleet, SetupError, never>(() => {
      if (this.#save.pending) return this.#save.retry();
      if (this.#continueSave) return this.#continueSave;
      if (this.#fleet) return this.#transport.call({path: `${this.#transport.fleets}/${this.#fleet.id}`}, FleetSchema);
      return Effect.fail(new ValidationError({detail: "No Save operation has started", suggestion: "use /review and /save first"}));
    }).pipe(Effect.tap((fleet) => Effect.sync(() => { this.#fleet = fleet; })));
  }

  apply(draft: Draft, review: Review, setupTag: string): Effect.Effect<Applied, SetupError> {
    return Effect.suspend<Applied, SetupError, never>(() => {
      if (this.#save.pending) return unresolved();
      if (!draft.candidate || !draft.linked_fleet_id) return missingCandidate();
      return this.applyCandidate(draft.candidate.id, draft.linked_fleet_id, review.review_digest, setupTag);
    });
  }

  applyCandidate(candidateId: string, fleetId: string, reviewDigest: string, setupTag: string) {
    return Effect.suspend<Applied, SetupError, never>(() => {
      if (this.#save.pending) return unresolved();
      const path = `${this.#transport.fleets}/${encodeURIComponent(fleetId)}/revisions`;
      const body = {draft_candidate_id: candidateId, review_digest: reviewDigest, origin: "cli"};
      return this.#apply.run({path, body, setupTag}, (key) => {
        this.#recovery = {action: SETUP_ACTION.apply, operation_id: key, draft_candidate_id: candidateId,
          fleet_id: fleetId, review_digest: reviewDigest, setup_etag: setupTag};
        return this.#transport.call({path, body, method: HTTP_METHOD.post,
          headers: {[IF_MATCH]: setupTag, [IDEMPOTENCY_KEY]: key}, retry: null}, AppliedSchema);
      }).pipe(Effect.ensuring(Effect.sync(() => { if (!this.#apply.pending) this.#recovery = undefined; })));
    });
  }

  fleet(id: string) { return this.#transport.call({path: `${this.#transport.fleets}/${id}`}, FleetSchema); }

  retryApply() { return this.#apply.retry(); }
}

function missingCandidate(): Effect.Effect<never, ValidationError> {
  return Effect.fail(new ValidationError({detail: "This action requires a validated chat candidate",
    suggestion: "continue chatting, then use /review before saving or applying"}));
}

function unresolved(): Effect.Effect<never, ValidationError> {
  return Effect.fail(new ValidationError({detail: "Another setup operation has an unknown outcome", suggestion: "use /retry before another setup action"}));
}
