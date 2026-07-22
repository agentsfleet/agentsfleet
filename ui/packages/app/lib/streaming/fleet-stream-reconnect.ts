// The reconnect policy for a live event stream: how fast to retry, when a
// retry is pending, and the two browser signals that warrant retrying
// immediately. Pure policy plus Document Object Model (DOM) wiring — the
// registry owns the connection state and passes the pieces this module needs
// through a narrow interface, so nothing here can reach into an entry it does
// not understand.

// How many fast attempts run before the connection is reported as not live.
// Reporting is all that changes at this point — the client keeps trying.
export const FAST_RECONNECT_ATTEMPTS = 5;
// The unhurried cadence a not-live connection keeps trying on. An outage that
// outlasts the fast attempts is usually minutes long, not seconds, and the
// operator should not have to press a button to come back from it.
export const OFFLINE_RETRY_MS = 30_000;

const RECONNECT_BACKOFF_BASE_MS = 1_000;
const RECONNECT_BACKOFF_CAP_MS = 15_000;
const RECONNECT_MAX_BACKOFF_ATTEMPTS = 5;

// Equal jitter: scale a computed delay by a random factor in [0.5, 1.0). Many
// browsers reconnecting to one recovered upstream must not fire in lockstep and
// stampede it; jitter spreads them. The factor only ever SHORTENS the delay
// below its ceiling, so a caller waiting the full ceiling always sees the retry.
export function jitter(ms: number): number {
  return Math.round(ms * (0.5 + Math.random() * 0.5));
}

/** Exponential backoff for the fast attempts, capped, with jitter applied. */
export function fastBackoffMs(attempts: number): number {
  const capped = Math.min(
    RECONNECT_BACKOFF_BASE_MS * 2 ** Math.min(attempts, RECONNECT_MAX_BACKOFF_ATTEMPTS),
    RECONNECT_BACKOFF_CAP_MS,
  );
  return jitter(capped);
}

type HoldsReconnectTimer = { reconnectTimer: ReturnType<typeof setTimeout> | null };

// One place cancels a pending reconnect, so the "is one pending?" question is
// asked identically by the operator's retry, the recovery signals, and
// teardown.
export function cancelPendingReconnect(holder: HoldsReconnectTimer): void {
  if (holder.reconnectTimer) clearTimeout(holder.reconnectTimer);
  holder.reconnectTimer = null;
}

export type RecoveryHooks = {
  /** True while a connection is open or already being attempted. */
  hasConnection: () => boolean;
  /** Drop any pending wait and open a fresh connection now. */
  recover: () => void;
};

/**
 * The two moments a stale connection is both most likely wrong and cheapest to
 * re-establish: the tab coming back to the foreground, and the browser
 * reporting the network back. Either one skips the remaining wait. Returns the
 * detach function; the caller runs it at teardown.
 */
export function attachRecoveryListeners(hooks: RecoveryHooks): () => void {
  const onSignal = () => {
    // A connection already open or in flight needs nothing; this is what keeps
    // both signals arriving together from opening two streams.
    if (hooks.hasConnection()) return;
    // `visibilitychange` fires on the way out as well as in. Reconnecting for
    // a tab nobody is looking at spends a stream slot on nothing.
    if (document.visibilityState === "hidden") return;
    hooks.recover();
  };
  document.addEventListener("visibilitychange", onSignal);
  window.addEventListener("online", onSignal);
  return () => {
    document.removeEventListener("visibilitychange", onSignal);
    window.removeEventListener("online", onSignal);
  };
}
