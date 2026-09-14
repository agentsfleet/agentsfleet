// Named keepalives share the data path with activity. A successful HTTP open
// alone proves neither liveness nor recovery.
export const HEARTBEAT_EVENT = "heartbeat";
const STABLE_CONNECTION_MS = 30_000;
const RENEWAL_GRACE_MS = 25_000;
const CONNECTION_ATTEMPT_TIMEOUT_MS = 30_000;
// Three missed 15-second server keepalives tolerate ordinary scheduling jitter.
const STREAM_SILENCE_TIMEOUT_MS = 45_000;

export class StreamRecoveryWindow {
  #firstArrival: number | null = null;
  #stable = false;
  #deadline: number | null = null;
  #noticeTimer: ReturnType<typeof setTimeout> | null = null;
  #connectionTimer: ReturnType<typeof setTimeout> | null = null;

  connecting(onTimeout: () => void): void {
    this.#arm(CONNECTION_ATTEMPT_TIMEOUT_MS, onTimeout);
  }

  opened(onTimeout: () => void): void {
    this.#firstArrival = null;
    this.#stable = false;
    this.#arm(STREAM_SILENCE_TIMEOUT_MS, onTimeout);
  }

  received(onTimeout: () => void): void {
    this.#clearNotice();
    const now = performance.now();
    this.#firstArrival ??= now;
    this.#stable = now - this.#firstArrival >= STABLE_CONNECTION_MS;
    this.#arm(STREAM_SILENCE_TIMEOUT_MS, onTimeout);
  }

  isStable(): boolean {
    return this.#stable;
  }

  isStale(): boolean {
    return this.#deadline !== null && performance.now() >= this.#deadline;
  }

  reportLoss(report: () => void): void {
    this.#clearConnection();
    const stable = this.#stable;
    this.#firstArrival = null;
    this.#stable = false;
    // Repeated failures and HTTP opens must not restart the grace deadline.
    if (this.#noticeTimer !== null) return;
    if (!stable) {
      report();
      return;
    }
    this.#noticeTimer = setTimeout(() => {
      this.#noticeTimer = null;
      report();
    }, RENEWAL_GRACE_MS);
  }

  dispose(): void {
    this.#clearConnection();
    this.#clearNotice();
    this.#firstArrival = null;
    this.#stable = false;
  }

  #arm(delay: number, onTimeout: () => void): void {
    this.#clearConnection();
    this.#deadline = performance.now() + delay;
    this.#connectionTimer = setTimeout(() => {
      this.#connectionTimer = null;
      onTimeout();
    }, delay);
  }

  #clearNotice(): void {
    if (this.#noticeTimer !== null) clearTimeout(this.#noticeTimer);
    this.#noticeTimer = null;
  }

  #clearConnection(): void {
    if (this.#connectionTimer !== null) clearTimeout(this.#connectionTimer);
    this.#connectionTimer = null;
    this.#deadline = null;
  }
}
