// A lost answer retains its identity. Changed input cannot masquerade as retry.

import { mintOperationId } from "@/lib/streaming/operation-id";
import { definiteRefusal, SetupUnavailable } from "./draft-request";

type Pending = {readonly identity: string; readonly execute: () => Promise<void>};

export class DraftOperation {
  #pending: Pending | undefined;
  #running = false;

  get pending(): boolean { return this.#pending !== undefined; }
  clear(): void { this.#pending = undefined; }

  async run(identity: unknown, execute: (key: string) => Promise<void>): Promise<void> {
    const fingerprint = JSON.stringify(identity);
    if (this.#pending && this.#pending.identity !== fingerprint) {
      throw new SetupUnavailable("Resolve the unconfirmed operation with Retry before changing its input.", 409);
    }
    if (!this.#pending) {
      const key = mintOperationId();
      this.#pending = {identity: fingerprint, execute: () => execute(key)};
    }
    await this.retry();
  }

  async retry(): Promise<void> {
    if (this.#running) throw new SetupUnavailable("This request is still in progress.", 409);
    const pending = this.#pending;
    if (!pending) throw new SetupUnavailable("There is no unconfirmed operation to retry.", 409);
    this.#running = true;
    try {
      await pending.execute();
      if (this.#pending === pending) this.#pending = undefined;
    } catch (cause) {
      if (definiteRefusal(cause) && this.#pending === pending) this.#pending = undefined;
      throw cause;
    } finally { this.#running = false; }
  }
}
