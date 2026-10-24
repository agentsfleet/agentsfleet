import React from "react";
import { afterEach, beforeEach, expect, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { generateEphemeralKeypair } from "@/lib/auth/cli-flow";
import type { LoopbackHandoff } from "@/lib/auth/cli-loopback";
import { captureBrowserLoginFragment, readBrowserHandoff } from "@/lib/auth/cli-loopback-store";

interface Person { id: string; primaryEmailAddress: { emailAddress: string } | null; fullName: string | null }
const external = vi.hoisted(() => ({
  getToken: vi.fn<(options?: { template: string }) => Promise<string | null>>(),
  clerk: { user: null as Person | null, session: null as { id: string } | null },
  auth: { isLoaded: true, isSignedIn: true },
}));
vi.mock("@clerk/nextjs", () => ({
  useAuth: () => ({ ...external.auth, getToken: external.getToken }),
  useClerk: () => external.clerk,
}));
import CliAuthPage from "@/app/cli-auth/[session_id]/page";

export const { getToken, clerk, auth } = external;

export const USER_ID = "person-a";
export const BROWSER_ID = "browser-a";
export const OTHER_ID = "other";
export const SESSION_ID = "0190b3c0-1111-7000-8000-000000000001";
export const ROLE = { button: "button", alert: "alert" } as const;
export const AUTHORIZE = "Authorize";
export const DENY = "Deny";
export const JSON_TYPE = "application/json";
export const KEY_FORMAT = "base64url";
const API_TEMPLATE = "api";
const NOW_WINDOW_MS = 300_000;
const TOKEN_FIXTURE_MARKER = "fixture";
export let terminal: Awaited<ReturnType<typeof generateEphemeralKeypair>>;
export let handoff: LoopbackHandoff;
export let fetchMock: ReturnType<typeof vi.fn<(url: string, init?: RequestInit) => Promise<Response>>>;
export let submissions: Record<string, FormDataEntryValue>[];

export function encodedToken(claims: unknown): string {
  return [btoa(JSON.stringify({ alg: TOKEN_FIXTURE_MARKER })), btoa(JSON.stringify(claims)), TOKEN_FIXTURE_MARKER].join(".");
}

export function jwt(sub: string, sid?: string): string {
  return encodedToken({ sub, ...(sid ? { sid } : {}) });
}

export function activeBody(overrides: Record<string, unknown> = {}) {
  return { status: "pending", cli_public_key: handoff.publicKey,
    expires_at: new Date(Date.now() + NOW_WINDOW_MS).toISOString(), ...overrides };
}

export function installHandoff(next: LoopbackHandoff = handoff) {
  handoff = next;
  const fragment = new URLSearchParams({ v: "1", session_id: handoff.sessionId, cli_public_key: handoff.publicKey,
    callback: handoff.callback, state: handoff.state });
  window.history.replaceState(null, "", `/cli-auth/${handoff.sessionId}?flow=loopback#${fragment.toString()}`);
  captureBrowserLoginFragment();
}

export async function renderAuth(sessionId: string = SESSION_ID) {
  const params = Promise.resolve({ session_id: sessionId });
  const element = () => <React.Suspense fallback={null}><CliAuthPage params={params} /></React.Suspense>;
  const frames: ReturnType<typeof render>[] = [];
  await act(async () => { frames.push(render(element())); });
  const mounted = frames[0];
  if (!mounted) throw new Error("Expected the real approval page to mount");
  return { ...mounted, refresh: () => mounted.rerender(element()) };
}

export async function show(body: unknown = activeBody()) {
  fetchMock.mockResolvedValueOnce(Response.json(body)).mockResolvedValue(new Response(null));
  const mounted = await renderAuth();
  await screen.findByLabelText("Verification code");
  return mounted;
}

export async function clickAuthorize() {
  await act(async () => { fireEvent.click(screen.getByRole(ROLE.button, { name: AUTHORIZE })); });
}

export async function expectRefused() {
  await screen.findByRole(ROLE.alert);
  expect(submissions).toEqual([]);
  expect((screen.getByRole(ROLE.button, { name: AUTHORIZE }) as HTMLButtonElement).disabled).toBe(true);
  expect(readBrowserHandoff(SESSION_ID).kind).toBe("invalid");
}

beforeEach(async () => {
  terminal = await generateEphemeralKeypair();
  handoff = { sessionId: SESSION_ID, publicKey: terminal.publicKeyBase64Url,
    callback: "http://127.0.0.1:49213/cli-auth/callback",
    state: Buffer.from(crypto.getRandomValues(new Uint8Array(32))).toString(KEY_FORMAT) };
  window.sessionStorage.clear();
  installHandoff();
  submissions = [];
  auth.isLoaded = true;
  auth.isSignedIn = true;
  getToken.mockReset().mockImplementation(async options =>
    options?.template === API_TEMPLATE ? jwt(USER_ID) : jwt(USER_ID, BROWSER_ID));
  clerk.user = { id: USER_ID, primaryEmailAddress: { emailAddress: "person@example.test" }, fullName: "Person" };
  clerk.session = { id: BROWSER_ID };
  fetchMock = vi.fn<(url: string, init?: RequestInit) => Promise<Response>>();
  vi.stubGlobal("fetch", fetchMock);
  vi.spyOn(HTMLFormElement.prototype, "submit").mockImplementation(function (this: HTMLFormElement) {
    expect(this.method.toUpperCase()).toBe("POST");
    expect(this.action).toBe(handoff.callback);
    expect(this.enctype).toBe("application/x-www-form-urlencoded");
    expect(this.target).toBe("_self");
    expect(this.classList.contains("ph-no-capture")).toBe(true);
    expect(this.classList.contains("ph-no-record")).toBe(true);
    submissions.push(Object.fromEntries(new FormData(this)));
  });
});
afterEach(() => { cleanup(); vi.unstubAllGlobals(); vi.restoreAllMocks(); });
