import {
  hasLoopbackFragment, LOOPBACK_FAILURE, LOOPBACK_FLOW, LOOPBACK_FLOW_PARAM, LOOPBACK_KIND,
  LOOPBACK_MAX_AGE_MS, LOOPBACK_RECORD_BYTES, parseLoopbackFragment,
  type BrowserHandoff, type LoopbackHandoff,
} from "./cli-loopback";

const STORAGE_KEY = "agentsfleet.cli.loopback.v1";
const MAX_RECORDS = 2;
const RETURN_PARAM = "redirect_url";
const AUTH_PATH = /^\/sign-(?:in|up)(?:\/|$)/;
const SESSION_PATH = /^\/cli-auth\/([0-9a-f-]{36})$/;
const OBJECT_TYPE = "object";

interface StoredHandoff { fragment: string; createdAt: number }
interface ValidatedRecord { record: StoredHandoff; handoff: LoopbackHandoff }
interface BrowserPort {
  url(): URL;
  replace(url: URL): void;
  storage(): Storage;
  now(): number;
}

const browserPort: BrowserPort = {
  url: () => new URL(window.location.href),
  replace: (url) => window.history.replaceState(window.history.state, "", url),
  storage: () => window.sessionStorage,
  now: () => Date.now(),
};

function markedSession(url: URL): string | null {
  const sessionId = SESSION_PATH.exec(url.pathname)?.[1];
  return sessionId && url.searchParams.getAll(LOOPBACK_FLOW_PARAM).length === 1 &&
    url.searchParams.get(LOOPBACK_FLOW_PARAM) === LOOPBACK_FLOW ? sessionId : null;
}

function captureSession(url: URL): string | null {
  if (!AUTH_PATH.test(url.pathname)) return markedSession(url);
  const destinationValue = url.searchParams.get(RETURN_PARAM);
  if (destinationValue === null || url.searchParams.getAll(RETURN_PARAM).length !== 1) return null;
  try {
    const destination = new URL(destinationValue, url.origin);
    return destination.origin === url.origin && !destination.hash && !destination.username && !destination.password
      ? markedSession(destination) : null;
  } catch { return null; }
}

function decodeRecord(value: unknown, now: number): ValidatedRecord | null {
  if (typeof value !== OBJECT_TYPE || value === null || Array.isArray(value)) return null;
  const record = value as Record<string, unknown>;
  if (Object.keys(record).length !== 2 || typeof record.fragment !== "string" || typeof record.createdAt !== "number") return null;
  if (!Number.isSafeInteger(record.createdAt) || record.createdAt < 0 || record.createdAt > now) return null;
  if (new TextEncoder().encode(JSON.stringify(record)).length > LOOPBACK_RECORD_BYTES) return null;
  const handoff = parseLoopbackFragment(record.fragment);
  return handoff ? { record: { fragment: record.fragment, createdAt: record.createdAt }, handoff } : null;
}

/** Transient navigation continuity only; the terminal and API enforce expiry. */
export class LoopbackStore {
  readonly #port: BrowserPort;
  #failure: { sessionId: string | null } | null = null;

  constructor(port: BrowserPort = browserPort) { this.#port = port; }

  #records(): ValidatedRecord[] {
    const raw = this.#port.storage().getItem(STORAGE_KEY);
    if (raw === null) return [];
    if (raw.length > MAX_RECORDS * LOOPBACK_RECORD_BYTES) throw new Error(LOOPBACK_FAILURE);
    const values: unknown = JSON.parse(raw);
    if (!Array.isArray(values) || values.length > MAX_RECORDS) throw new Error(LOOPBACK_FAILURE);
    const valid: ValidatedRecord[] = [];
    for (const value of values) {
      const record = decodeRecord(value, this.#port.now());
      if (record === null) throw new Error(LOOPBACK_FAILURE);
      valid.push(record);
    }
    const ids = valid.map(({ handoff }) => handoff.sessionId);
    if (new Set(ids).size !== ids.length) throw new Error(LOOPBACK_FAILURE);
    const active = valid.filter(({ record }) => this.#port.now() - record.createdAt < LOOPBACK_MAX_AGE_MS);
    if (active.length !== valid.length) {
      if (active.length === 0) this.#port.storage().removeItem(STORAGE_KEY);
      else this.#port.storage().setItem(STORAGE_KEY, JSON.stringify(active.map(({ record }) => record)));
    }
    return active;
  }

  capture(): void {
    const url = this.#port.url();
    if (!SESSION_PATH.test(url.pathname) && !AUTH_PATH.test(url.pathname)) return;
    if (!url.hash) return;
    const marked = captureSession(url);
    if (!marked && !hasLoopbackFragment(url.hash)) return;
    const fragment = url.hash;
    url.hash = "";
    try {
      this.#port.replace(url);
      const handoff = parseLoopbackFragment(fragment);
      if (!marked || !handoff || handoff.sessionId !== marked) throw new Error(LOOPBACK_FAILURE);
      this.#save(fragment, marked);
      this.#failure = null;
    } catch {
      if (marked) this.clear(marked);
      this.#failure = { sessionId: marked ?? SESSION_PATH.exec(url.pathname)?.[1] ?? null };
    }
  }

  #save(fragment: string, sessionId: string): void {
    const records = this.#records();
    const existing = records.find(({ handoff }) => handoff.sessionId === sessionId);
    if (existing && existing.record.fragment !== fragment) throw new Error(LOOPBACK_FAILURE);
    if (existing) return;
    if (records.length === MAX_RECORDS) throw new Error(LOOPBACK_FAILURE);
    const record = { fragment, createdAt: this.#port.now() };
    if (new TextEncoder().encode(JSON.stringify(record)).length > LOOPBACK_RECORD_BYTES) throw new Error(LOOPBACK_FAILURE);
    this.#port.storage().setItem(STORAGE_KEY, JSON.stringify([...records.map((entry) => entry.record), record]));
  }

  read(sessionId: string): BrowserHandoff {
    const url = this.#port.url();
    if (this.#failure && (this.#failure.sessionId === null || this.#failure.sessionId === sessionId)) return this.#invalid();
    if (!url.searchParams.has(LOOPBACK_FLOW_PARAM)) return { kind: LOOPBACK_KIND.manual };
    if (markedSession(url) !== sessionId) return this.#invalid();
    try {
      const handoff = this.#records().find((entry) => entry.handoff.sessionId === sessionId)?.handoff;
      return handoff ? { kind: LOOPBACK_KIND.automatic, handoff } : this.#invalid();
    } catch {
      this.clear(sessionId);
      return this.#invalid();
    }
  }

  clear(sessionId: string): void {
    try {
      const records = this.#records().filter((entry) => entry.handoff.sessionId !== sessionId);
      if (records.length === 0) this.#port.storage().removeItem(STORAGE_KEY);
      else this.#port.storage().setItem(STORAGE_KEY, JSON.stringify(records.map(({ record }) => record)));
    } catch {
      try { this.#port.storage().removeItem(STORAGE_KEY); } catch { /* Storage can be disabled. */ }
    }
    if (this.#failure?.sessionId === sessionId) this.#failure = null;
  }

  #invalid(): BrowserHandoff { return { kind: LOOPBACK_KIND.invalid, message: LOOPBACK_FAILURE }; }
}

const loopbackStore = new LoopbackStore();
export const captureBrowserLoginFragment = (): void => loopbackStore.capture();
export const readBrowserHandoff = (sessionId: string): BrowserHandoff => loopbackStore.read(sessionId);
export const clearBrowserHandoff = (sessionId: string): void => loopbackStore.clear(sessionId);
