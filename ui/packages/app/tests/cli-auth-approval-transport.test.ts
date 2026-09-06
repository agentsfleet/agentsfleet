import React from "react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";

const { getToken, encryptJwt } = vi.hoisted(() => ({
  getToken: vi.fn(),
  encryptJwt: vi.fn(),
}));
vi.mock("@clerk/nextjs", () => ({
  useAuth: () => ({ isLoaded: true, isSignedIn: true, getToken }),
}));
vi.mock("@/lib/auth/cli-flow", () => ({
  generateEphemeralKeypair: async () => ({ privateKey: {}, publicKeyBase64Url: "public-key" }),
  deriveSharedKey: async () => ({}),
  encryptJwt,
  generateVerificationCode: () => "123456",
}));
import CliAuthPage from "@/app/cli-auth/[session_id]/page";

const SESSION_TOKEN = "browser-session-token";
const HANDOFF_TOKEN = "terminal-handoff-token";

beforeEach(() => {
  getToken.mockReset().mockImplementation(async (options?: { template: string }) =>
    options ? HANDOFF_TOKEN : SESSION_TOKEN);
  encryptJwt.mockReset().mockResolvedValue({ ciphertext: "encrypted", nonce: "nonce" });
});
afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

async function approve(response: Response) {
  const fetchMock = vi.fn()
    .mockResolvedValueOnce(Response.json({
      status: "pending", cli_public_key: "peer-key", token_name: "terminal",
      expires_at_ms: Date.now() + 300_000,
    }))
    .mockResolvedValueOnce(response);
  vi.stubGlobal("fetch", fetchMock);
  await act(async () => {
    render(React.createElement(React.Suspense, { fallback: null },
      React.createElement(CliAuthPage, { params: Promise.resolve({ session_id: "session-id" }) })));
  });
  const button = await screen.findByRole("button", { name: "Approve" });
  await act(async () => { fireEvent.click(button); });
  return fetchMock;
}

it("authorizes the same-origin approval with the session token and encrypts only the handoff token", async () => {
  const fetchMock = await approve(new Response(null, { status: 200 }));
  expect(await screen.findByLabelText("Verification code")).toBeTruthy();
  expect(getToken).toHaveBeenCalledWith();
  expect(getToken.mock.calls[1]?.[0]?.template).toBe("api");
  expect(encryptJwt).toHaveBeenCalledWith(HANDOFF_TOKEN, expect.anything());
  const request = fetchMock.mock.calls[1]?.[1] as RequestInit;
  expect(request.headers).toEqual(expect.objectContaining({ Authorization: `Bearer ${SESSION_TOKEN}` }));
  expect(request.body).not.toContain(SESSION_TOKEN);
  expect(request.body).not.toContain(HANDOFF_TOKEN);
});

it("does not display an approval code for a redirected successful response", async () => {
  const redirected = new Response("sign-in page", { status: 200 });
  Object.defineProperty(redirected, "redirected", { value: true });
  await approve(redirected);
  expect(await screen.findByText(/dashboard session expired/i)).toBeTruthy();
  expect(screen.queryByLabelText("Verification code")).toBeNull();
});

it("does not send the approval if the browser session token is unavailable", async () => {
  getToken.mockImplementation(async (options?: { template: string }) => options ? HANDOFF_TOKEN : null);
  const fetchMock = await approve(new Response(null, { status: 200 }));
  expect(await screen.findByText(/dashboard session expired/i)).toBeTruthy();
  expect(fetchMock).toHaveBeenCalledTimes(1);
  expect(screen.queryByLabelText("Verification code")).toBeNull();
});

it.each([null, new Error("template unavailable")])("does not approve when the handoff token mint returns %s", async (result) => {
  getToken.mockImplementation(async (options?: { template: string }) => {
    if (!options) return SESSION_TOKEN;
    if (result instanceof Error) throw result;
    return result;
  });
  const fetchMock = await approve(new Response(null, { status: 200 }));
  const message = result instanceof Error ? /Something went wrong/i : /dashboard session expired/i;
  expect(await screen.findByText(message)).toBeTruthy();
  expect(fetchMock).toHaveBeenCalledTimes(1);
  expect(encryptJwt).not.toHaveBeenCalled();
  expect(screen.queryByLabelText("Verification code")).toBeNull();
});
