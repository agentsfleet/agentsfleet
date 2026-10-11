import { expect, it } from "bun:test";
import { request } from "playwright";
import { vercelBypassState } from "./vercel-bypass.ts";

const HOST = "127.0.0.1";
const BYPASS_HEADER = "x-vercel-protection-bypass";
const SET_COOKIE_HEADER = "x-vercel-set-bypass-cookie";
const COOKIE_NAME = "fixture-bypass";
const COOKIE_PATH = "/";
const HTTP_FOUND = 302;

it("should preserve only the derived cookie on subsequent requests", async () => {
  const secret = crypto.randomUUID();
  const cookie = crypto.randomUUID();
  const seen: Array<{ bypass: string | null; setCookie: string | null; cookie: string | null }> = [];
  const server = Bun.serve({ hostname: HOST, port: 0, fetch(req) {
    seen.push({ bypass: req.headers.get(BYPASS_HEADER), setCookie: req.headers.get(SET_COOKIE_HEADER), cookie: req.headers.get("cookie") });
    return new Response(null, { headers: { "Set-Cookie": `${COOKIE_NAME}=${cookie}; Path=${COOKIE_PATH}; HttpOnly` } });
  } });
  try {
    const state = await vercelBypassState(server.url.origin, secret);
    expect(state.origins).toEqual([]);
    expect(state.cookies).toHaveLength(1);
    expect(state.cookies[0]).toMatchObject({ name: COOKIE_NAME, value: cookie, domain: HOST, path: COOKIE_PATH, httpOnly: true });
    const context = await request.newContext({ storageState: state });
    try { await context.get(server.url.origin); } finally { await context.dispose(); }
    expect(seen).toEqual([
      { bypass: secret, setCookie: "true", cookie: null },
      { bypass: null, setCookie: null, cookie: `${COOKIE_NAME}=${cookie}` },
    ]);
  } finally { await server.stop(true); }
});

it("should never forward the bypass secret to a redirect target", async () => {
  const secret = crypto.randomUUID();
  let targetRequests = 0;
  const target = Bun.serve({ hostname: HOST, port: 0, fetch() {
    targetRequests++;
    return new Response(null);
  } });
  let originRequests = 0;
  const origin = Bun.serve({ hostname: HOST, port: 0, fetch(req) {
    originRequests++;
    expect(req.headers.get(BYPASS_HEADER)).toBe(secret);
    return new Response(null, { status: HTTP_FOUND, headers: { Location: target.url.origin } });
  } });
  try {
    const state = await vercelBypassState(origin.url.origin, secret);
    expect(state).toEqual({ cookies: [], origins: [] });
    expect(originRequests).toBe(1);
    expect(targetRequests).toBe(0);
  } finally {
    await origin.stop(true);
    await target.stop(true);
  }
});

it("should avoid network access when deployment protection is absent", async () => {
  expect(await vercelBypassState("invalid origin")).toEqual({ cookies: [], origins: [] });
});

it("should surface connection refusal without returning an empty success state", async () => {
  const server = Bun.serve({ hostname: HOST, port: 0, fetch() { return new Response(null); } });
  const origin = server.url.origin;
  await server.stop(true);
  await expect(vercelBypassState(origin, crypto.randomUUID())).rejects.toThrow("ECONNREFUSED");
});
