import { attachRecoveryListeners, fastBackoffMs, FAST_RECONNECT_ATTEMPTS, OFFLINE_RETRY_MS } from "./fleet-stream-reconnect";
import { HEARTBEAT_EVENT, StreamRecoveryWindow } from "./stream-recovery-window";

export const CHAT_CONNECTION_STATUS = {
  CONNECTING: "connecting", LIVE: "live", RECONNECTING: "reconnecting", OFFLINE: "offline", REVOKED: "revoked",
} as const;
export type ChatConnectionStatus = typeof CHAT_CONNECTION_STATUS[keyof typeof CHAT_CONNECTION_STATUS];
type Options = {
  readonly url: string;
  readonly events: readonly string[];
  readonly onFrame: (name: string, data: string) => boolean;
  readonly onOpen: (recovered: boolean) => void;
  readonly onInterrupted: () => void;
  readonly onStatus?: (status: ChatConnectionStatus) => void;
  readonly onHeartbeat?: () => void;
};

/** One owned connection, silence deadline and reconnect policy for either chat purpose. */
export class ChatConnection {
  readonly #window = new StreamRecoveryWindow();
  #source: EventSource | null = null;
  #retryTimer: ReturnType<typeof setTimeout> | null = null;
  #detach: (() => void) | null = null;
  #attempts = 0;
  #opened = false;
  #interrupted = false;
  #closed = false;
  #status: ChatConnectionStatus = CHAT_CONNECTION_STATUS.CONNECTING;

  constructor(private readonly options: Options) {}
  #isDisposed(): boolean { return this.#closed; }

  start(): void {
    if (this.#closed || this.#source !== null) return;
    if (typeof document !== "undefined" && typeof window !== "undefined") {
      this.#detach ??= attachRecoveryListeners({
        hasConnection: () => this.#source !== null && !this.#window.isStale(),
        recover: () => this.#recover(),
      });
    }
    this.#open();
  }

  retry(): void {
    if (this.#closed) return;
    if (this.#source !== null) this.options.onInterrupted();
    if (this.#isDisposed()) return;
    this.#interrupted ||= this.#source !== null && this.#opened;
    this.#disposeSource();
    this.#attempts = 0;
    this.#report(CHAT_CONNECTION_STATUS.CONNECTING);
    this.#open();
  }

  close(): void {
    this.#closed = true;
    this.#disposeSource();
    this.#detach?.(); this.#detach = null;
  }

  #recover(): void {
    if (this.#source) {this.options.onInterrupted(); this.#interrupted = true;}
    if (this.#isDisposed()) return;
    if (this.#retryTimer !== null) clearTimeout(this.#retryTimer);
    this.#retryTimer = null;
    this.#source?.close(); this.#source = null;
    // Browser wake accelerates the existing recovery; it does not renew its grace period.
    if (this.#status !== CHAT_CONNECTION_STATUS.LIVE) this.#report(CHAT_CONNECTION_STATUS.CONNECTING);
    this.#open();
  }

  #open(): void {
    if (this.#closed) return;
    let source: EventSource;
    try { source = new EventSource(this.options.url); }
    catch { this.#failed(); return; }
    this.#source = source;
    const current = (): boolean => !this.#closed && this.#source === source;
    const fail = (): void => { if (current()) this.#failed(); };
    source.onopen = () => {
      if (!current()) return;
      this.#window.opened(fail);
      const recovered = this.#opened || this.#interrupted;
      this.#opened = true; this.#interrupted = false;
      this.options.onOpen(recovered);
    };
    const frame = (name: string, data: string): void => {
      if (!current()) return;
      if (this.options.onFrame(name, data) && current()) this.#received(fail);
    };
    for (const name of this.options.events) {
      source.addEventListener(name, (event: MessageEvent<string>) => frame(name, event.data));
    }
    source.onmessage = (event) => frame("message", event.data);
    source.addEventListener(HEARTBEAT_EVENT, () => {
      if (!current()) return;
      this.#received(fail);
      if (current()) this.options.onHeartbeat?.();
    });
    source.onerror = fail;
    this.#window.connecting(fail);
  }

  #received(fail: () => void): void {
    this.#window.received(fail);
    if (this.#window.isStable()) this.#attempts = 0;
    this.#report(CHAT_CONNECTION_STATUS.LIVE);
  }

  #failed(): void {
    this.options.onInterrupted();
    if (this.#isDisposed()) return;
    this.#source?.close(); this.#source = null;
    this.#interrupted = true;
    if (this.#window.isStable()) this.#attempts = 0;
    this.#attempts += 1;
    this.#window.reportLoss(() => this.#report(this.#attempts > FAST_RECONNECT_ATTEMPTS
      ? CHAT_CONNECTION_STATUS.OFFLINE : CHAT_CONNECTION_STATUS.RECONNECTING));
    if (this.#isDisposed()) return;
    this.#retryTimer = setTimeout(() => {
      this.#retryTimer = null; this.#open();
    }, this.#attempts > FAST_RECONNECT_ATTEMPTS ? OFFLINE_RETRY_MS : fastBackoffMs(this.#attempts));
  }

  #report(status: ChatConnectionStatus): void {
    this.#status = status;
    this.options.onStatus?.(status);
  }

  #disposeSource(): void {
    if (this.#retryTimer !== null) clearTimeout(this.#retryTimer);
    this.#retryTimer = null;
    this.#window.dispose();
    this.#source?.close(); this.#source = null;
  }
}
