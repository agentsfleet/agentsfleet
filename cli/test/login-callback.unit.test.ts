import { afterEach, describe, expect, spyOn, test } from "bun:test";
import { LoginCallback } from "../src/commands/login-callback.ts";
import { CALLBACK_DECISION, CALLBACK_STATUS, MAX_CALLBACK_BODY_BYTES } from "../src/commands/login-callback-protocol.ts";
import { HTTP_METHOD } from "../src/constants/http-method.ts";

const DASHBOARD_ORIGIN = "https://dashboard.test";
const SESSION_ID = "callback-test-session";
const VERIFICATION_CODE = "123456";
const BODY_TYPE = "application/x-www-form-urlencoded";
const MAX_PENDING_READS = 8;
const BARRIER_ATTEMPTS = 100;
const callbacks = new Set<LoginCallback>();

function listener(options: { deadlineMs?: number; requestTimeoutMs?: number } = {}): LoginCallback {
  const callback = new LoginCallback({ sessionId: SESSION_ID, dashboardOrigin: DASHBOARD_ORIGIN, ...options });
  callbacks.add(callback);
  return callback;
}

function approvedBody(callback: LoginCallback): URLSearchParams {
  return new URLSearchParams({ state: callback.state, session_id: SESSION_ID, status: CALLBACK_STATUS.approved, verification_code: VERIFICATION_CODE });
}

function deniedBody(callback: LoginCallback): URLSearchParams {
  return new URLSearchParams({ state: callback.state, session_id: SESSION_ID, status: CALLBACK_STATUS.denied });
}

function send(callback: LoginCallback, options: { body?: string | URLSearchParams; method?: string; origin?: string | null; headers?: Record<string, string>; suffix?: string } = {}): Promise<Response> {
  const headers = new Headers({ "Content-Type": BODY_TYPE, ...options.headers });
  if (options.origin !== null) headers.set("Origin", options.origin ?? DASHBOARD_ORIGIN);
  const method = options.method ?? HTTP_METHOD.post;
  return fetch(`${callback.callbackUrl}${options.suffix ?? ""}`, {
    method, headers,
    ...(method === HTTP_METHOD.get ? {} : { body: options.body ?? approvedBody(callback) }),
  });
}

async function denyAfterRefusal(callback: LoginCallback): Promise<void> {
  expect(callback.waiting).toBe(true);
  const response = await send(callback, { body: deniedBody(callback) });
  expect(response.status).toBe(200);
  expect(await callback.waitForDecision()).toEqual({ status: CALLBACK_STATUS.denied });
  expect(await response.text()).toContain("Authorization denied");
}

afterEach(async () => { await Promise.all([...callbacks].map((callback) => callback.close())); callbacks.clear(); });

describe("local authorization request validation", () => {
  test.each([
    ["missing origin", { origin: null }],
    ["null origin", { origin: "null" }],
    ["untrusted origin", { origin: "https://attacker.test" }],
    ["duplicate origin", { origin: `${DASHBOARD_ORIGIN}, ${DASHBOARD_ORIGIN}` }],
    ["forged host", { headers: { Host: "attacker.test" } }],
    ["wrong content type", { headers: { "Content-Type": "application/json" } }],
    ["wrong method", { method: HTTP_METHOD.get }],
    ["query string", { suffix: "?state=ignored" }],
    ["wrong path", { suffix: "/extra" }],
    ["oversized headers", { headers: { "X-Extra": "a".repeat(4_096) } }],
  ] as const)("rejects %s without consuming the waiting decision", async (_name, options) => {
    const callback = listener();
    const response = await send(callback, { body: deniedBody(callback), ...options });
    expect(response.status).toBe(400);
    expect(await response.text()).toBe("This terminal cannot accept that authorization. Start a new sign-in.");
    await denyAfterRefusal(callback);
  });

  test.each([
    ["wrong state", (body: URLSearchParams) => { body.set("state", "a".repeat(43)); body.set("status", CALLBACK_STATUS.denied); body.delete("verification_code"); }],
    ["non-ASCII state", (body: URLSearchParams) => body.set("state", "é".repeat(43))],
    ["wrong session", (body: URLSearchParams) => body.set("session_id", "other-session")],
    ["duplicate field", (body: URLSearchParams) => body.append("state", "duplicate")],
    ["unknown field", (body: URLSearchParams) => body.set("redirect", "https://attacker.test")],
    ["missing code", (body: URLSearchParams) => body.delete("verification_code")],
    ["malformed code", (body: URLSearchParams) => body.set("verification_code", "1é3456")],
    ["unknown decision", (body: URLSearchParams) => body.set("status", "unknown")],
    ["denial with code", (body: URLSearchParams) => body.set("status", CALLBACK_STATUS.denied)],
  ] as const)("rejects %s then still accepts a valid denial", async (_name, mutate) => {
    const callback = listener();
    const body = approvedBody(callback);
    mutate(body);
    expect((await send(callback, { body })).status).toBe(400);
    await denyAfterRefusal(callback);
  });

  test("bounds a complete oversized body before any decision", async () => {
    const callback = listener();
    const response = await send(callback, { body: "a".repeat(MAX_CALLBACK_BODY_BYTES + 1) });
    expect(response.status).toBe(413);
    await denyAfterRefusal(callback);
  });
});

describe("one authorization decision and actual final outcome", () => {
  test("holds the response until finalization succeeds and never echoes secrets", async () => {
    const callback = listener();
    let replied = false;
    const reply = send(callback).then((response) => { replied = true; return response; });
    expect(await callback.waitForDecision()).toEqual({ status: CALLBACK_STATUS.approved, verificationCode: VERIFICATION_CODE });
    expect(replied).toBe(false);
    expect(callback.cancel()).toBe(false);
    expect((await send(callback)).status).toBe(409);
    callback.finish(true);
    const response = await reply;
    const html = await response.text();
    expect(html).toContain("Terminal authorized");
    for (const secret of [callback.state, SESSION_ID, VERIFICATION_CODE]) expect(html).not.toContain(secret);
    expect(response.headers.get("Cache-Control")).toBe("no-store");
    expect(response.headers.get("Referrer-Policy")).toBe("no-referrer");
    expect(response.headers.get("Content-Security-Policy")).toContain("default-src 'none'");
  });

  test("a failed finalization reports failure and cannot later turn into success", async () => {
    const callback = listener();
    const reply = send(callback);
    await callback.waitForDecision();
    callback.finish(false);
    callback.finish(true);
    expect(await (await reply).text()).toContain("Sign-in did not complete");
    expect((await send(callback)).status).toBe(409);
  });

  test("one of one hundred concurrent approvals wins and replay never releases another decision", async () => {
    const callback = listener();
    const requests = Array.from({ length: 100 }, () => send(callback));
    expect((await callback.waitForDecision()).status).toBe(CALLBACK_STATUS.approved);
    callback.finish(true);
    const responses = await Promise.all(requests);
    expect(responses.filter((response) => response.ok)).toHaveLength(1);
    expect(responses.filter((response) => !response.ok)).toHaveLength(99);
    for (const response of responses) await response.text();
  });

  test("cancellation wins before approval and cannot be replaced by denial", async () => {
    const callback = listener();
    expect(callback.cancel()).toBe(true);
    expect(callback.cancel()).toBe(false);
    expect(await callback.waitForDecision()).toEqual({ status: CALLBACK_DECISION.canceled });
    expect((await send(callback)).status).toBe(409);
    await callback.close();
    await expect(fetch(callback.callbackUrl)).rejects.toThrow();
  });

  test("expiry refuses a later valid code and closes cleanly", async () => {
    const callback = listener({ deadlineMs: Date.now() - 1 });
    expect(await callback.waitForDecision()).toEqual({ status: CALLBACK_DECISION.expired });
    expect((await send(callback)).status).toBe(409);
    await callback.close();
    await expect(fetch(callback.callbackUrl)).rejects.toThrow();
  });

  test("lost browser response cannot cause a second exchange decision", async () => {
    const callback = listener();
    const abort = new AbortController();
    const pending = fetch(callback.callbackUrl, { method: HTTP_METHOD.post, headers: { Origin: DASHBOARD_ORIGIN }, body: approvedBody(callback), signal: abort.signal }).catch(() => null);
    expect((await callback.waitForDecision()).status).toBe(CALLBACK_STATUS.approved);
    abort.abort();
    await pending;
    expect((await send(callback)).status).toBe(409);
    callback.finish(true);
  });

  test("closing an unfinished finalization releases a failure response", async () => {
    const callback = listener();
    const pending = send(callback);
    await callback.waitForDecision();
    const closing = callback.close();
    expect(await (await pending).text()).toContain("Sign-in did not complete");
    await closing;
    await expect(fetch(callback.callbackUrl)).rejects.toThrow();
  });
});

async function slowRequest(callback: LoginCallback): Promise<string> {
  const result = Promise.withResolvers<string>();
  const chunks: Buffer[] = [];
  const url = new URL(callback.callbackUrl);
  const socket = await Bun.connect({
    hostname: url.hostname, port: Number(url.port),
    socket: {
      open(peer) {
        peer.write(`POST ${url.pathname} HTTP/1.1\r\nHost: ${url.host}\r\nOrigin: ${DASHBOARD_ORIGIN}\r\nContent-Type: ${BODY_TYPE}\r\nTransfer-Encoding: chunked\r\n\r\n7\r\nstate=a\r\n`);
      },
      data(_peer, data) { chunks.push(Buffer.from(data)); },
      close() { result.resolve(Buffer.concat(chunks).toString()); },
      error(_peer, error) { result.reject(error); },
    },
  });
  const timer = setTimeout(() => { socket.end(); result.reject(new Error("slow body did not reach its deadline")); }, 1_000);
  try { return await result.promise; } finally { clearTimeout(timer); socket.end(); }
}

async function pendingCallback(callback: LoginCallback) {
  const response = Promise.withResolvers<string>();
  const chunks: Buffer[] = [];
  const url = new URL(callback.callbackUrl);
  const body = approvedBody(callback).toString();
  const socket = await Bun.connect({
    hostname: url.hostname, port: Number(url.port),
    socket: {
      open(peer) {
        peer.write(`POST ${url.pathname} HTTP/1.1\r\nHost: ${url.host}\r\nOrigin: ${DASHBOARD_ORIGIN}\r\nContent-Type: ${BODY_TYPE}\r\nTransfer-Encoding: chunked\r\n\r\n1\r\n${body[0]}\r\n`);
      },
      data(_peer, data) { chunks.push(Buffer.from(data)); },
      close() { response.resolve(Buffer.concat(chunks).toString()); },
      error(_peer, error) { response.reject(error); },
    },
  });
  void response.promise.catch(() => {});
  return {
    reply: response.promise,
    complete: () => { const rest = body.slice(1); socket.write(`${Buffer.byteLength(rest).toString(16)}\r\n${rest}\r\n0\r\n\r\n`); },
    close: () => socket.end(),
  };
}

test("simultaneous streamed approvals recheck waiting after their bodies finish", async () => {
  const callback = listener();
  const peers = await Promise.all(Array.from({ length: MAX_PENDING_READS }, () => pendingCallback(callback)));
  try {
    let pendingLimitObserved = false;
    for (let attempt = 0; attempt < BARRIER_ATTEMPTS; attempt += 1) {
      const probe = await send(callback, { body: "" });
      await probe.text();
      if (probe.status === 429) { pendingLimitObserved = true; break; }
      expect(probe.status).toBe(400);
    }
    expect(pendingLimitObserved).toBe(true);
    for (const peer of peers) peer.complete();
    expect((await callback.waitForDecision()).status).toBe(CALLBACK_STATUS.approved);
    callback.finish(true);
    const replies = await Promise.all(peers.map((peer) => peer.reply));
    expect(replies.filter((reply) => reply.startsWith("HTTP/1.1 200"))).toHaveLength(1);
    expect(replies.filter((reply) => reply.startsWith("HTTP/1.1 409"))).toHaveLength(MAX_PENDING_READS - 1);
  } finally { for (const peer of peers) peer.close(); }
});

test("an incomplete streamed body reaches its deadline without consuming the slot", async () => {
  const callback = listener({ requestTimeoutMs: 25 });
  const response = await slowRequest(callback);
  expect(response).toContain("HTTP/1.1 400");
  await denyAfterRefusal(callback);
});

test("repeated fresh listeners bind once and release their ports on all waiting exits", async () => {
  for (let index = 0; index < 20; index += 1) {
    const callback = listener();
    const url = new URL(callback.callbackUrl);
    expect(url.hostname).toBe("127.0.0.1");
    expect(Number(url.port)).toBeGreaterThan(0);
    callback.cancel();
    await callback.close();
    await expect(fetch(callback.callbackUrl)).rejects.toThrow();
  }
});

test.each(["https://dashboard.test/path", "https://dashboard.test/", "ftp://dashboard.test"])("refuses configured dashboard values that are not trusted HTTP origins: %s", (origin) => {
  expect(() => new LoginCallback({ sessionId: SESSION_ID, dashboardOrigin: origin })).toThrow("invalid configured dashboard origin");
});

test("invalid UTF-8 is refused without consuming authorization or reflecting the body", async () => {
  const callback = listener();
  const response = await fetch(callback.callbackUrl, { method: HTTP_METHOD.post,
    headers: { Origin: DASHBOARD_ORIGIN, "Content-Type": BODY_TYPE }, body: new Uint8Array([0xff, 0xfe]) });
  expect(response.status).toBe(400);
  expect(await response.text()).toBe("This terminal cannot accept that authorization. Start a new sign-in.");
  await denyAfterRefusal(callback);
});

test("shutdown terminates active incomplete requests and releases the listener instead of waiting for their body deadline", async () => {
  const callback = listener();
  const peers = await Promise.all(Array.from({ length: MAX_PENDING_READS }, () => pendingCallback(callback)));
  try {
    let saturated = false;
    for (let attempt = 0; attempt < BARRIER_ATTEMPTS; attempt += 1) {
      const probe = await send(callback, { body: "" }); await probe.text();
      if (probe.status === 429) { saturated = true; break; }
    }
    expect(saturated).toBe(true);
    await callback.close();
    expect(await callback.waitForDecision()).toEqual({ status: CALLBACK_DECISION.canceled });
    const replies = await Promise.all(peers.map((peer) => peer.reply));
    expect(replies.every((reply) => !reply.startsWith("HTTP/1.1 200"))).toBe(true);
    await expect(fetch(callback.callbackUrl)).rejects.toThrow();
    await denyAfterRefusal(listener());
  } finally { for (const peer of peers) peer.close(); }
});

test("platform header failure returns a static refusal and preserves a later valid decision", async () => {
  const callback = listener();
  const fault = spyOn(Headers.prototype, Symbol.iterator).mockImplementationOnce(() => { throw new Error("private platform diagnostic"); });
  try {
    const response = await send(callback, { body: deniedBody(callback) });
    expect(response.status).toBe(400);
    expect(await response.text()).toBe("This terminal cannot accept that authorization. Start a new sign-in.");
    expect(response.headers.get("Cache-Control")).toBe("no-store");
  } finally { fault.mockRestore(); }
  await denyAfterRefusal(callback);
});

test("a failed stream cancellation at the body deadline remains a refusal and does not consume authorization", async () => {
  const callback = listener({ requestTimeoutMs: 25 });
  const fault = spyOn(ReadableStreamDefaultReader.prototype, "cancel")
    .mockImplementationOnce(() => Promise.reject(new Error("private cancellation diagnostic")));
  try {
    const response = await slowRequest(callback);
    expect(fault).toHaveBeenCalled();
    expect(response).toContain("HTTP/1.1 400");
    expect(response).not.toContain("private cancellation diagnostic");
  } finally { fault.mockRestore(); }
  await denyAfterRefusal(callback);
});
