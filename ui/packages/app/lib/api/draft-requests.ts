import type { DraftOperation } from "./draft-operation";
import type { DraftRequestState, DraftView } from "./draft-state";
import { draftRequestView } from "./draft-state";
import { SetupUnavailable } from "./draft-request";

/** One private client owns its request lifecycle, including unconfirmed retries. */
export class DraftRequests {
  #state: DraftRequestState = {kind: "idle"};
  readonly #owner: AbortSignal;
  readonly #operation: DraftOperation;
  readonly #changed: (patch: Partial<DraftView>) => void;
  readonly #failed: (cause: unknown) => void;

  constructor(owner: AbortSignal, operation: DraftOperation, changed: (patch: Partial<DraftView>) => void, failed: (cause: unknown) => void) {
    this.#owner = owner; this.#operation = operation; this.#changed = changed; this.#failed = failed;
  }

  get view(): ReturnType<typeof draftRequestView> { return draftRequestView(this.#state); }
  clear(): void { if (this.#state.kind !== "inaccessible") this.#state = {kind: "idle"}; }
  blockAccess(): void { this.#state = {kind: "inaccessible"}; }

  async run<T>(action: () => Promise<T>, setupAction: DraftView["setupAction"] = null): Promise<T> {
    const view = this.view;
    if (view.inaccessible) throw new SetupUnavailable("Reload this page to check access before continuing.", 403);
    if (view.busy) throw new SetupUnavailable("A setup request is still in progress.", 409);
    const request = {kind: this.#operation.pending ? "retrying" : "requesting", action: (this.#operation.pending ? view.setupAction : setupAction) ?? null} as const;
    this.#state = request;
    this.#changed({error: null, notice: null});
    try { return await action(); }
    catch (cause) {
      if (!this.#owner.aborted && this.#state === request) this.#failed(cause);
      throw cause;
    } finally {
      if (this.#state === request) {
        this.#state = this.#operation.pending ? {kind: "unconfirmed", action: request.action} : {kind: "idle"};
        this.#changed({});
      }
    }
  }
}
