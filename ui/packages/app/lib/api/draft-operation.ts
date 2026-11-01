// A lost answer retains its identity. Changed input cannot masquerade as retry.

import { mintOperationId } from "@/lib/streaming/operation-id";
import { definiteRefusal, SetupUnavailable } from "./draft-request";

type Pending = {readonly identity: string; readonly execute: () => Promise<void>};
type State = {readonly kind: "idle"} | {readonly kind: "running" | "unconfirmed"; readonly pending: Pending};

export class DraftOperation {
  #state: State = {kind: "idle"};

  get pending(): boolean { return this.#state.kind !== "idle"; }
  clear(): void { this.#state = {kind: "idle"}; }

  async run(identity: unknown, execute: (key: string) => Promise<void>): Promise<void> {
    const fingerprint = JSON.stringify(identity);
    const pending = this.#state.kind === "idle" ? undefined : this.#state.pending;
    if (pending && pending.identity !== fingerprint) {
      throw new SetupUnavailable("Resolve the unconfirmed operation with Retry before changing its input.", 409);
    }
    if (!pending) {
      const key = mintOperationId();
      this.#state = {kind: "unconfirmed", pending: {identity: fingerprint, execute: () => execute(key)}};
    }
    await this.retry();
  }

  async retry(): Promise<void> {
    const state = this.#state;
    if (state.kind === "running") throw new SetupUnavailable("This request is still in progress.", 409);
    if (state.kind === "idle") throw new SetupUnavailable("There is no unconfirmed operation to retry.", 409);
    const running = {kind: "running", pending: state.pending} as const;
    this.#state = running;
    try {
      await running.pending.execute();
      if (this.#state === running) this.clear();
    } catch (cause) {
      if (this.#state === running) this.#state = definiteRefusal(cause) ? {kind: "idle"} : {kind: "unconfirmed", pending: running.pending};
      throw cause;
    }
  }
}
