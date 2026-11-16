import React from "react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { generateEphemeralKeypair } from "@/lib/auth/cli-flow";

const { getToken } = vi.hoisted(() => ({ getToken: vi.fn() }));
vi.mock("@clerk/nextjs", () => ({
  useAuth: () => ({ isLoaded: true, isSignedIn: true, getToken }),
}));
import CliAuthPage from "@/app/cli-auth/[session_id]/page";

const SESSION_ID = "0190b3c0-1111-7000-8000-000000000001";
const HEADING = "Unleash your agents.";
let expiresAt: string;
let poll: ReturnType<typeof vi.fn<() => Promise<Response>>>;
let requests: { url: string; init?: RequestInit }[];

beforeEach(() => {
  vi.useFakeTimers({ shouldAdvanceTime: true });
  expiresAt = new Date(Date.now() + 300_000).toISOString();
  poll = vi.fn().mockImplementation(async () => Response.json({ status: "verification_pending" }));
  requests = [];
  getToken.mockReset().mockResolvedValue("browser-session-fixture");
  window.history.replaceState(null, "", `/cli-auth/${SESSION_ID}`);
  window.sessionStorage.clear();
});

afterEach(() => { cleanup(); vi.useRealTimers(); vi.unstubAllGlobals(); });

async function approve() {
  const terminal = await generateEphemeralKeypair();
  vi.stubGlobal("fetch", vi.fn(async (url: string, init?: RequestInit) => {
    requests.push({ url, init });
    if (init?.method === "PATCH") return new Response(null, { status: 200 });
    if (requests.length === 1) return Response.json({ status: "pending", cli_public_key: terminal.publicKeyBase64Url,
      token_name: "terminal", expires_at: expiresAt });
    return poll();
  }));
  const params = Promise.resolve({ session_id: SESSION_ID });
  const mounts: ReturnType<typeof render>[] = [];
  await act(async () => { mounts.push(render(<React.Suspense fallback={null}><CliAuthPage params={params} /></React.Suspense>)); });
  const mounted = mounts[0];
  if (!mounted) throw new Error("Expected the manual approval page to mount");
  const button = await screen.findByRole("button", { name: "Authorize" });
  await act(async () => { fireEvent.click(button); });
  expect(await screen.findByLabelText("Verification code")).toBeTruthy();
  return mounted;
}

async function readStatus() { await act(async () => { await vi.advanceTimersByTimeAsync(2_401); }); }

it("shows completion only after the terminal consumes the verification code", async () => {
  await approve();
  const code = screen.getByLabelText("Verification code").textContent;
  await readStatus();
  expect(screen.getByLabelText("Verification code").textContent).toBe(code);
  expect(screen.queryByRole("heading", { name: HEADING })).toBeNull();
  poll.mockResolvedValueOnce(Response.json({ error_code: "UZ-AUTH-012" }, { status: 410 }));
  await readStatus();
  expect(screen.getByRole("heading", { name: HEADING })).toBeTruthy();
  expect(screen.getByText("Your terminal is authorized. Return to your terminal to finish signing in.")).toBeTruthy();
  expect(screen.queryByText(/You're signed in/)).toBeNull();
  expect(screen.getByText("You can close this tab.")).toBeTruthy();
  expect(screen.queryByLabelText("Verification code")).toBeNull();
  const calls = poll.mock.calls.length;
  await readStatus();
  expect(poll).toHaveBeenCalledTimes(calls);
});

it.each([{ status: 410, code: "UZ-AUTH-013" }, { status: 401, code: "UZ-AUTH-006" },
  { status: 404, code: "UZ-AUTH-005" }, { status: 410, code: "unexpected" }])(
  "does not report a refused session as successful: $status / $code", async ({ status, code }) => {
    await approve();
    poll.mockResolvedValueOnce(Response.json({ error_code: code }, { status }));
    await readStatus();
    expect(screen.queryByRole("heading", { name: HEADING })).toBeNull();
    expect(screen.getByText(/This sign-in could not be confirmed/)).toBeTruthy();
    expect(screen.queryByLabelText("Verification code")).toBeNull();
  });

it.each([
  { name: "successful HTTP status", response: () => Response.json({ error_code: "UZ-AUTH-012" }) },
  { name: "server failure", response: () => Response.json({ error_code: "UZ-AUTH-012" }, { status: 503 }) },
  { name: "malformed body", response: () => new Response("{", { status: 410 }) },
  { name: "empty body", response: () => new Response(null, { status: 410 }) },
  { name: "oversized body", response: () => Response.json({ error_code: "UZ-AUTH-012", detail: "x".repeat(4_097) }, { status: 410 }) },
])("does not trust a consumed code on the wrong response: $name", async ({ response }) => {
  await approve();
  poll.mockImplementation(async () => response());
  await readStatus();
  expect(screen.queryByRole("heading", { name: HEADING })).toBeNull();
  expect(screen.getByLabelText("Verification code")).toBeTruthy();
});

it("keeps the code after a network failure and observes a later completion", async () => {
  await approve();
  poll.mockRejectedValueOnce(new TypeError("network unavailable"));
  await readStatus();
  expect(screen.getByLabelText("Verification code")).toBeTruthy();
  poll.mockResolvedValueOnce(Response.json({ error_code: "UZ-AUTH-012" }, { status: 410 }));
  await readStatus();
  expect(screen.getByRole("heading", { name: HEADING })).toBeTruthy();
});

it("aborts an outstanding completion read on unmount", async () => {
  const mounted = await approve();
  poll.mockReturnValue(new Promise<Response>(() => undefined));
  await readStatus();
  const signal = requests.at(-1)?.init?.signal;
  expect(signal?.aborted).toBe(false);
  mounted.unmount();
  expect(signal?.aborted).toBe(true);
  const calls = poll.mock.calls.length;
  await readStatus();
  expect(poll).toHaveBeenCalledTimes(calls);
});

it("stops observing when the login session expires", async () => {
  expiresAt = new Date(Date.now() + 1_000).toISOString();
  await approve();
  await readStatus();
  expect(screen.getByText("This sign-in expired. Start a new sign-in from your terminal.")).toBeTruthy();
  expect(screen.queryByRole("heading", { name: HEADING })).toBeNull();
  expect(poll).not.toHaveBeenCalled();
});
