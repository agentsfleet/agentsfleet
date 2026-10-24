export type AnalyticsIdentity = { id: string; email?: string | null } | null;
export type AnalyticsIdentityReader = () => AnalyticsIdentity | undefined;
const NO_PROVIDER = Symbol("no analytics identity provider");

// An absent provider preserves standalone callers. An installed provider owns
// readiness: undefined means Clerk is unresolved or its committed effect expired.
export class AnalyticsIdentityGate {
  #reader: AnalyticsIdentityReader | null = null;
  #preparedId: string | null | undefined;
  #queueOwner: string | null | symbol | undefined;

  constructor(private readonly discardQueued: () => void) {}

  bind(reader?: AnalyticsIdentityReader): void {
    if (reader) { this.#reader = reader; this.#preparedId = undefined; }
  }

  ready(): boolean {
    return this.#owner() !== undefined;
  }

  #owner(): string | null | symbol | undefined {
    if (this.#reader === null) return NO_PROVIDER;
    try {
      const identity = this.#reader();
      return identity === undefined ? undefined : identity?.id ?? null;
    } catch { return undefined; }
  }

  // Unresolved callers cannot enqueue; a new owner replaces both telemetry queues.
  queueIsCurrent(): boolean {
    const owner = this.#owner();
    if (owner === undefined) return false;
    if (owner !== this.#queueOwner) {
      this.discardQueued();
      this.#queueOwner = owner;
    }
    return true;
  }

  allowsEvents(): boolean {
    if (this.#reader === null) return true;
    try {
      const identity = this.#reader();
      return identity !== undefined && this.#preparedId === (identity?.id ?? null);
    } catch { return false; }
  }

  prepare(sync: (identity: AnalyticsIdentity) => void): boolean {
    if (this.#reader === null) return true;
    try {
      const identity = this.#reader();
      if (identity === undefined) return false;
      // Permit the SDK's synchronous $identify event; a failed sync closes delivery.
      this.#preparedId = identity?.id ?? null;
      sync(identity);
      return true;
    } catch { this.#preparedId = undefined; return false; }
  }
}
