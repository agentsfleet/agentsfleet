// Whether the tab that owns a send is still alive, through the Web Locks API.
// A document holds a lock named for each operation it has in flight, from
// `begin` to the moment its ending is written, and the browser releases it
// when that document goes. A tab that reads another tab's send as `sending`
// asks for the same lock: being granted it means nobody is sending that
// operation any more, so the send can be read as unknown and its Resend
// shown. No heartbeat and no timer — the lock is the liveness.
//
// The owner requests its lock before it writes the `sending` entry that makes
// other tabs look, so its request is queued first. A watcher granted early
// anyway shows Resend over a live send, which is safe: the Resend carries the
// same operation id and the daemon answers the first admission.
//
// Without `navigator.locks` every method does nothing and `supported()` says
// so: a foreign `sending` then stays hidden until a reload or its expiry. A
// manager that is there but refuses requests — a `SecurityError` in an opaque
// origin, an `InvalidStateError` in a document that is not fully active — is
// the same browser without locks: from its first refusal `supported()` says
// no, and the watch it refused reads its send as a reload would, unknown.

type LockRequester = Pick<LockManager, "request">;

/** Every send's lock name starts here; the operation id completes it. */
export const SEND_LOCK_PREFIX = "agentsfleet:pending-send:";

export class SendLocks {
  // The release for each lock this document holds, keyed by operation id.
  #held = new Map<string, () => void>();
  // Operation ids this document is waiting to be granted.
  #watched = new Set<string>();
  // The manager refused a request: this document asks it nothing more.
  #refused = false;

  supported(): boolean {
    return this.#manager() !== null;
  }

  /** Holds `operationId`'s lock until `release`. */
  hold(operationId: string): void {
    const locks = this.#manager();
    if (locks === null || this.#held.has(operationId)) return;
    // The executor runs now, so a release that comes before the grant still
    // finds its resolver and the lock is let go the moment it is granted.
    const released = new Promise<void>((resolve) => this.#held.set(operationId, resolve));
    locks.request(lockName(operationId), () => released).catch(() => {
      this.#refused = true;
    });
  }

  release(operationId: string): void {
    this.#held.get(operationId)?.();
    this.#held.delete(operationId);
  }

  /** Calls `gone` once no document holds `operationId`'s lock, or once the
   * manager refuses to say. */
  watch(operationId: string, gone: () => void): void {
    const locks = this.#manager();
    if (locks === null || this.#held.has(operationId) || this.#watched.has(operationId)) return;
    this.#watched.add(operationId);
    const granted = (): void => {
      this.#watched.delete(operationId);
      gone();
    };
    // A watch already granted rejects only for what `gone` threw, which is
    // not the manager refusing.
    locks.request(lockName(operationId), granted).catch(() => {
      if (!this.#watched.has(operationId)) return;
      this.#refused = true;
      granted();
    });
  }

  /** Lets every held lock go and forgets every watch and refusal — for a test's reset. */
  clear(): void {
    for (const release of this.#held.values()) release();
    this.#held.clear();
    this.#watched.clear();
    this.#refused = false;
  }

  #manager(): LockRequester | null {
    return this.#refused ? null : lockManager();
  }
}

function lockName(operationId: string): string {
  return `${SEND_LOCK_PREFIX}${operationId}`;
}

// Read per call rather than once: a test installs its own manager, and a
// browser that throws on the getter answers as one without locks.
function lockManager(): LockRequester | null {
  try {
    const locks = (globalThis as { navigator?: { locks?: Partial<LockRequester> } }).navigator?.locks;
    return typeof locks?.request === "function" ? (locks as LockRequester) : null;
  } catch {
    return null;
  }
}
