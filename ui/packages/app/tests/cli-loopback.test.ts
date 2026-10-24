import { createHash, randomBytes, randomUUID, webcrypto } from "node:crypto";
import { beforeAll, beforeEach, describe, expect, it, vi } from "vitest";
import {
  CALLBACK_PATH, LOOPBACK_FAILURE, LOOPBACK_FLOW, LOOPBACK_KIND, LOOPBACK_MAX_AGE_MS,
  LOOPBACK_VERSION, matchingCode, parseLoopbackFragment,
} from "../lib/auth/cli-loopback";
import { captureBrowserLoginFragment, clearBrowserHandoff, readBrowserHandoff, LoopbackStore } from "../lib/auth/cli-loopback-store";

const ORIGIN = "https://app.example.test";
const START = 1_000_000;
const STORAGE_KEY = "agentsfleet.cli.loopback.v1";
let publicKey: string;

beforeAll(async () => {
  const pair = await webcrypto.subtle.generateKey({ name: "ECDH", namedCurve: "P-256" }, true, ["deriveBits"]);
  publicKey = Buffer.from(await webcrypto.subtle.exportKey("spki", pair.publicKey)).toString("base64url");
});
beforeEach(() => { window.sessionStorage.clear(); });

function fixture(sessionId = randomUUID()) {
  const params = new URLSearchParams({
    v: LOOPBACK_VERSION, session_id: sessionId, cli_public_key: publicKey,
    callback: `http://127.0.0.1:54321${CALLBACK_PATH}`, state: randomBytes(32).toString("base64url"),
  });
  return { sessionId, params, fragment: `#${params}`, path: `/cli-auth/${sessionId}?flow=${LOOPBACK_FLOW}` };
}

function harness(initial: URL) {
  let url = initial;
  let now = START;
  const storage = window.sessionStorage;
  const replace = vi.fn((next: URL) => { url = new URL(next); });
  const port = { url: () => url, replace, storage: () => storage, now: () => now };
  return { store: new LoopbackStore(port), storage, replace, port,
    navigate: (next: string) => { url = new URL(next, ORIGIN); },
    advance: (milliseconds: number) => { now += milliseconds; },
    current: () => url,
  };
}

describe("local login protocol", () => {
  it("binds the comparison code to the full key and session using the pinned transcript", async () => {
    const input = fixture();
    const handoff = parseLoopbackFragment(input.fragment)!;
    const expected = createHash("sha256").update(`agentsfleet.cli.login.loopback.v1\n${input.sessionId}\n${publicKey}`)
      .digest("hex").slice(0, 8).toUpperCase();
    expect(handoff).toEqual({ sessionId: input.sessionId, publicKey, callback: input.params.get("callback"), state: input.params.get("state") });
    expect(await matchingCode(handoff)).toBe(`${expected.slice(0, 4)}-${expected.slice(4)}`);
    expect(await matchingCode({ ...handoff, sessionId: randomUUID() })).not.toBe(await matchingCode(handoff));
  });

  it.each([
    "http://localhost:54321/cli-auth/callback", "http://127.1:54321/cli-auth/callback",
    "http://2130706433:54321/cli-auth/callback", "http://[::1]:54321/cli-auth/callback",
    "https://127.0.0.1:54321/cli-auth/callback", "http://127.0.0.1:0/cli-auth/callback",
    "http://127.0.0.1:65536/cli-auth/callback", "http://127.0.0.1:054321/cli-auth/callback",
    "http://127.0.0.1:54321/cli-auth/callback?code=x", "http://127.0.0.1:54321/cli-auth/callback#x",
    "http://user@127.0.0.1:54321/cli-auth/callback", "http://127.0.0.1:54321/cli-auth/../cli-auth/callback",
  ])("refuses nonliteral or ambiguous callback %s", (callback) => {
    const { params } = fixture(); params.set("callback", callback);
    expect(parseLoopbackFragment(`#${params}`)).toBeNull();
  });

  it.each(["v", "session_id", "cli_public_key", "callback", "state"])("refuses missing and repeated %s", (field) => {
    const { params } = fixture(); params.append(field, params.get(field)!);
    expect(parseLoopbackFragment(`#${params}`)).toBeNull();
    params.delete(field);
    expect(parseLoopbackFragment(`#${params}`)).toBeNull();
  });

  it.each([["v", "2"], ["state", "short"], ["session_id", "../other"], ["cli_public_key", "invalid"], ["unknown", "field"]])(
    "refuses malformed %s", (field, value) => {
      const { params } = fixture(); params.set(field, value);
      expect(parseLoopbackFragment(`#${params}`)).toBeNull();
    });

  it("refuses padded and noncanonical state, malformed escapes and oversized fragments", () => {
    const input = fixture();
    input.params.set("state", `${input.params.get("state") }=`);
    expect(parseLoopbackFragment(`#${input.params}`)).toBeNull();
    expect(parseLoopbackFragment(`${input.fragment}%GG`)).toBeNull();
    expect(parseLoopbackFragment(`${input.fragment}${"x".repeat(2_048)}`)).toBeNull();
    input.params.set("cli_public_key", randomBytes(91).toString("base64url"));
    expect(parseLoopbackFragment(`#${input.params}`)).toBeNull();
  });

  it("refuses duplicate fields whose total count matches the required shape", () => {
    const { params } = fixture(); params.delete("v"); params.append("state", params.get("state")!);
    expect(parseLoopbackFragment(`#${params}`)).toBeNull();
  });

  it("refuses a correct-length key with the wrong algorithm while state remains valid", () => {
    const { params } = fixture();
    params.set("cli_public_key", randomBytes(91).toString("base64url"));
    expect(parseLoopbackFragment(`#${params}`)).toBeNull();
  });

  it("refuses invalid base64 length and nonzero unused bits", () => {
    const { params } = fixture(); params.set("state", "a");
    expect(parseLoopbackFragment(`#${params}`)).toBeNull();
    const state = randomBytes(32).toString("base64url");
    const alphabet = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    params.set("state", state.slice(0, -1) + alphabet[alphabet.indexOf(state.at(-1)!) + 1]);
    expect(parseLoopbackFragment(`#${params}`)).toBeNull();
  });
});

describe("browser handoff lifetime", () => {
  it("uses the real browser storage and history through the public singleton", () => {
    const input = fixture();
    window.history.replaceState(null, "", `${input.path}${input.fragment}`);
    captureBrowserLoginFragment();
    expect(window.location.hash).toBe("");
    expect(readBrowserHandoff(input.sessionId).kind).toBe(LOOPBACK_KIND.automatic);
    clearBrowserHandoff(input.sessionId);
    expect(readBrowserHandoff(input.sessionId).kind).toBe(LOOPBACK_KIND.invalid);
    window.history.replaceState(null, "", "/");
  });
  it("strips before storage and restores across sign-in without extending the deadline", () => {
    const input = fixture();
    const signIn = new URL(`/sign-in?redirect_url=${encodeURIComponent(input.path)}${input.fragment}`, ORIGIN);
    const context = harness(signIn);
    context.store.capture();
    expect(context.current().hash).toBe("");
    expect(context.storage.length).toBe(1);
    context.navigate(input.path);
    expect(new LoopbackStore(context.port).read(input.sessionId).kind).toBe(LOOPBACK_KIND.automatic);
    context.advance(LOOPBACK_MAX_AGE_MS - 1);
    context.navigate(`${input.path}${input.fragment}`);
    context.store.capture();
    expect(context.store.read(input.sessionId).kind).toBe(LOOPBACK_KIND.automatic);
    context.advance(1);
    expect(context.store.read(input.sessionId)).toEqual({ kind: LOOPBACK_KIND.invalid, message: LOOPBACK_FAILURE });
    expect(context.storage.length).toBe(0);
  });

  it("does not consume unrelated provider state on ordinary sign-in", () => {
    const context = harness(new URL("/sign-in#state=provider-value&code=provider-code", ORIGIN));
    context.store.capture();
    expect(context.current().hash).toBe("#state=provider-value&code=provider-code");
    expect(context.storage.length).toBe(0);
    expect(context.replace).not.toHaveBeenCalled();
  });

  it("leaves fragment-free auth loads alone and refuses an unmarked callback fragment", () => {
    const input = fixture(); const context = harness(new URL(input.path, ORIGIN));
    context.store.capture(); expect(context.replace).not.toHaveBeenCalled();
    context.navigate("/sign-in#v=1&callback=bad"); context.store.capture();
    expect(context.current().hash).toBe("");
    context.navigate(input.path); expect(context.store.read(input.sessionId).kind).toBe(LOOPBACK_KIND.invalid);
  });

  it.each(["https://evil.example/cli-auth/", "/cli-auth/"])("refuses a mismatched sign-in destination %s", (prefix) => {
    const input = fixture();
    const destination = `${prefix}${randomUUID()}?flow=${LOOPBACK_FLOW}`;
    const context = harness(new URL(`/sign-in?redirect_url=${encodeURIComponent(destination)}${input.fragment}`, ORIGIN));
    context.store.capture();
    expect(context.current().hash).toBe("");
    expect(context.storage.length).toBe(0);
    context.navigate(input.path);
    expect(context.store.read(input.sessionId).kind).toBe(LOOPBACK_KIND.invalid);
  });

  it("distinguishes explicit manual mode from missing or invalid automatic state", () => {
    const input = fixture(); const context = harness(new URL(input.path, ORIGIN));
    expect(context.store.read(input.sessionId).kind).toBe(LOOPBACK_KIND.invalid);
    context.navigate(input.path.replace(`?flow=${LOOPBACK_FLOW}`, ""));
    expect(context.store.read(input.sessionId).kind).toBe(LOOPBACK_KIND.manual);
    context.navigate(`${input.path}&flow=${LOOPBACK_FLOW}`);
    expect(context.store.read(input.sessionId).kind).toBe(LOOPBACK_KIND.invalid);
  });

  it("isolates two sessions, refuses a third, and clears only the completed session", () => {
    const first = fixture(); const second = fixture(); const third = fixture();
    const context = harness(new URL(`${first.path}${first.fragment}`, ORIGIN)); context.store.capture();
    context.navigate(`${second.path}${second.fragment}`); context.store.capture();
    context.navigate(`${third.path}${third.fragment}`); context.store.capture();
    expect(context.store.read(third.sessionId).kind).toBe(LOOPBACK_KIND.invalid);
    context.navigate(second.path);
    expect(context.store.read(second.sessionId).kind).toBe(LOOPBACK_KIND.automatic);
    context.store.clear(first.sessionId);
    expect(context.store.read(second.sessionId).kind).toBe(LOOPBACK_KIND.automatic);
    context.store.clear(second.sessionId);
    expect(context.storage.length).toBe(0);
  });

  it("rejects conflicting handoffs for the same session", () => {
    const first = fixture(); const context = harness(new URL(`${first.path}${first.fragment}`, ORIGIN));
    context.store.capture();
    const conflict = fixture(first.sessionId);
    context.navigate(`${conflict.path}${conflict.fragment}`); context.store.capture();
    expect(context.store.read(first.sessionId).kind).toBe(LOOPBACK_KIND.invalid);
    expect(context.storage.length).toBe(0);
  });

  it("expires an older session without discarding a newer concurrent login", () => {
    const first = fixture(); const second = fixture();
    const context = harness(new URL(`${first.path}${first.fragment}`, ORIGIN)); context.store.capture();
    context.advance(100);
    context.navigate(`${second.path}${second.fragment}`); context.store.capture();
    context.advance(LOOPBACK_MAX_AGE_MS - 100);
    expect(context.store.read(second.sessionId).kind).toBe(LOOPBACK_KIND.automatic);
    context.navigate(first.path);
    expect(context.store.read(first.sessionId).kind).toBe(LOOPBACK_KIND.invalid);
  });

  it.each(["get", "set", "replace"])("fails visibly when browser %s fails without retaining the fragment", (operation) => {
    const input = fixture(); const context = harness(new URL(`${input.path}${input.fragment}`, ORIGIN));
    const port = { ...context.port };
    if (operation === "replace") port.replace.mockImplementation(() => { throw new Error("blocked"); });
    else port.storage = () => new Proxy(context.storage, { get(target, property) {
      if (property === (operation === "get" ? "getItem" : "setItem")) return () => { throw new Error("blocked"); };
      const value = Reflect.get(target, property); return typeof value === "function" ? value.bind(target) : value;
    } });
    const store = new LoopbackStore(port); store.capture();
    expect(store.read(input.sessionId).kind).toBe(LOOPBACK_KIND.invalid);
    if (operation !== "replace") expect(context.current().hash).toBe("");
    expect(context.storage.length).toBe(0);
  });

  it.each(["not-json", "{}", "null", "[]", "x".repeat(5_000)])("refuses corrupt stored records", (raw) => {
    const input = fixture(); const context = harness(new URL(input.path, ORIGIN));
    context.storage.setItem(STORAGE_KEY, raw);
    expect(context.store.read(input.sessionId).kind).toBe(LOOPBACK_KIND.invalid);
  });

  it("revalidates stored key, duplicate sessions and future timestamps", () => {
    const input = fixture(); const context = harness(new URL(input.path, ORIGIN));
    const record = { fragment: input.fragment, createdAt: START };
    for (const records of [[{ ...record, createdAt: START + 1 }], [record, record], [{ ...record, extra: true }]]) {
      context.storage.setItem(STORAGE_KEY, JSON.stringify(records));
      expect(context.store.read(input.sessionId).kind).toBe(LOOPBACK_KIND.invalid);
    }
  });

  it.each([null, [], 1, { fragment: 1, createdAt: START }, { fragment: "invalid", createdAt: "now" },
    { fragment: "invalid", createdAt: START }, { fragment: "x".repeat(2_050), createdAt: START }])(
    "refuses invalid stored record %#", (record) => {
      const input = fixture(); const context = harness(new URL(input.path, ORIGIN));
      context.storage.setItem(STORAGE_KEY, JSON.stringify([record]));
      expect(context.store.read(input.sessionId).kind).toBe(LOOPBACK_KIND.invalid);
    });

  it("refuses excess persisted sessions and negative or fractional creation times", () => {
    const input = fixture(); const context = harness(new URL(input.path, ORIGIN));
    const record = { fragment: input.fragment, createdAt: START };
    for (const records of [[record, record, record], [{ ...record, createdAt: -1 }], [{ ...record, createdAt: 1.5 }]]) {
      context.storage.setItem(STORAGE_KEY, JSON.stringify(records));
      expect(context.store.read(input.sessionId).kind).toBe(LOOPBACK_KIND.invalid);
    }
  });

  it.each(["http://[", "/", "/cli-auth/invalid?flow=loopback"])("strips refused CLI fragments on invalid return %s", (destination) => {
    const input = fixture();
    const context = harness(new URL(`/sign-in?redirect_url=${encodeURIComponent(destination)}${input.fragment}`, ORIGIN));
    context.store.capture(); expect(context.current().hash).toBe("");
    context.navigate(input.path); expect(context.store.read(input.sessionId).kind).toBe(LOOPBACK_KIND.invalid);
  });

  it("strips incomplete marked state and unmarked distinctive fragments without downgrade", () => {
    const input = fixture(); const context = harness(new URL(`${input.path}#v=1&callback=bad`, ORIGIN));
    context.store.capture(); expect(context.current().hash).toBe("");
    expect(context.store.read(input.sessionId).kind).toBe(LOOPBACK_KIND.invalid);
    context.store.clear(input.sessionId);
    context.navigate(`/cli-auth/${input.sessionId}#cli_public_key=invalid`); context.store.capture();
    expect(context.current().hash).toBe("");
    expect(context.store.read(input.sessionId).kind).toBe(LOOPBACK_KIND.invalid);
  });

  it("refuses fragments whose serialized record exceeds the storage byte limit", () => {
    const input = fixture(); const padded = input.fragment.padEnd(2_040, "&");
    const context = harness(new URL(`${input.path}${padded}`, ORIGIN));
    context.store.capture(); expect(context.store.read(input.sessionId).kind).toBe(LOOPBACK_KIND.invalid);
    expect(context.storage.length).toBe(0);
  });

  it("contains storage access and removal failures during cleanup", () => {
    const input = fixture(); const context = harness(new URL(`${input.path}${input.fragment}`, ORIGIN));
    const store = new LoopbackStore({ ...context.port, storage: () => { throw new Error("disabled"); } });
    store.capture(); expect(context.current().hash).toBe("");
    expect(store.read(input.sessionId).kind).toBe(LOOPBACK_KIND.invalid);
    expect(() => store.clear(input.sessionId)).not.toThrow();
  });
});
