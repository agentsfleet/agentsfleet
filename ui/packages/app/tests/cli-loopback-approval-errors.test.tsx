import { expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, screen, waitFor } from "@testing-library/react";
import { readBrowserHandoff } from "@/lib/auth/cli-loopback-store";
import {
  AUTHORIZE, BROWSER_ID, DENY, OTHER_ID, ROLE, SESSION_ID, USER_ID,
  activeBody, auth, clerk, clickAuthorize, encodedToken, expectRefused, fetchMock, getToken,
  handoff, installHandoff, jwt, renderAuth, show, submissions,
} from "./helpers/cli-loopback-fixture";

const RESTART_TEXT = /Start a new sign-in from your terminal/;
const ACCOUNT_TEXT = /Your signed-in account changed/;
const VERIFY_LABEL = "Verification code";
const API_OPTIONS = { template: "api" };
const PENDING_STATUS = "pending";
const REDIRECTED_PROPERTY = "redirected";
const ENCRYPT_METHOD = "encrypt";
const FORM_TAG = "form";
const RESOLVED_OUTCOME = "resolve";
const RESPONSE_FAILURE = { redirect: "redirect", refused: "refused", bodyless: "bodyless", network: "network" } as const;
const IDENTITY = { account: "account", browser: "browser session", signedOut: "signed out", missingUser: "missing user",
  missingIdentity: "missing identity", missingSession: "missing session" } as const;

it.each([null, PENDING_STATUS])("refuses a non-object manual session response %j without obtaining credentials", async body => {
  window.history.replaceState(null, "", `/cli-auth/${SESSION_ID}`);
  fetchMock.mockResolvedValue(Response.json(body));
  await renderAuth();
  await screen.findByRole("heading", { name: "Login session unavailable" });
  expect(screen.queryByRole(ROLE.button, { name: AUTHORIZE })).toBeNull();
  expect(submissions).toEqual([]);
  expect(getToken).not.toHaveBeenCalled();
});

it.each([null, [], PENDING_STATUS, 7, { status: "approved" }, { expires_at: null }])(
  "refuses malformed or already consumed session data %j before obtaining a token", async body => {
    fetchMock.mockResolvedValue(Response.json(typeof body === "object" && body !== null && !Array.isArray(body)
      ? activeBody(body) : body));
    await renderAuth();
    await expectRefused();
    expect(screen.getByText(RESTART_TEXT)).toBeTruthy();
    expect(getToken).not.toHaveBeenCalled();
    expect(fetchMock).toHaveBeenCalledTimes(1);
  });

it.each([RESPONSE_FAILURE.redirect, RESPONSE_FAILURE.refused, RESPONSE_FAILURE.bodyless, "invalid JSON", RESPONSE_FAILURE.network])(
  "refuses a %s session lookup without sending a terminal decision", async failure => {
    const response = failure === RESPONSE_FAILURE.refused ? new Response(null, { status: 410 })
      : failure === RESPONSE_FAILURE.bodyless ? new Response(null) : new Response("not JSON");
    if (failure === RESPONSE_FAILURE.redirect) Object.defineProperty(response, REDIRECTED_PROPERTY, { value: true });
    if (failure === RESPONSE_FAILURE.network) fetchMock.mockRejectedValue(new Error("lookup offline"));
    else fetchMock.mockResolvedValue(response);
    await renderAuth();
    await expectRefused();
    expect(getToken).not.toHaveBeenCalled();
    expect(fetchMock).toHaveBeenCalledTimes(1);
  });

it("cancels an oversized streamed session before reading more bytes and releases its reader", async () => {
  const cancelled = vi.fn();
  const body = new ReadableStream<Uint8Array>({
    start(controller) { controller.enqueue(new Uint8Array(4_097)); }, cancel: cancelled,
  });
  fetchMock.mockResolvedValue(new Response(body));
  await renderAuth();
  await expectRefused();
  expect(cancelled).toHaveBeenCalledTimes(1);
  expect(body.locked).toBe(false);
  expect(getToken).not.toHaveBeenCalled();
});

it("releases a failed session stream without approving or leaving its reader locked", async () => {
  const body = new ReadableStream<Uint8Array>({ start(controller) { controller.error(new Error("stream failed")); } });
  fetchMock.mockResolvedValue(new Response(body));
  await renderAuth();
  await expectRefused();
  expect(body.locked).toBe(false);
  expect(getToken).not.toHaveBeenCalled();
});

it.each([
  "truncated", "header..signature", "header.%%%.signature", "header.bm90IGpzb24.signature",
  encodedToken(null), encodedToken([]), encodedToken(USER_ID), encodedToken({}),
  encodedToken({ sub: USER_ID }), encodedToken({ sub: USER_ID, sid: OTHER_ID }),
])("refuses malformed session tokens and missing browser identity claims %s", async token => {
  getToken.mockResolvedValue(token);
  await show();
  await clickAuthorize();
  await expectRefused();
  expect(screen.getByText(ACCOUNT_TEXT)).toBeTruthy();
  expect(fetchMock).toHaveBeenCalledTimes(1);
  expect(getToken).toHaveBeenCalledTimes(1);
});

it("refuses a malformed API token after acquiring a valid browser token", async () => {
  getToken.mockResolvedValueOnce(jwt(USER_ID, BROWSER_ID)).mockResolvedValue(encodedToken({ sid: BROWSER_ID }));
  await show();
  await clickAuthorize();
  await expectRefused();
  expect(getToken.mock.calls).toEqual([[], [API_OPTIONS]]);
  expect(fetchMock).toHaveBeenCalledTimes(1);
});

it.each([IDENTITY.account, IDENTITY.browser, IDENTITY.signedOut])(
  "refuses a changed %s before releasing the first token", async change => {
    await show();
    if (change === IDENTITY.account && clerk.user) clerk.user.id = OTHER_ID;
    if (change === IDENTITY.browser && clerk.session) clerk.session.id = OTHER_ID;
    if (change === IDENTITY.signedOut) clerk.user = null;
    await clickAuthorize();
    await expectRefused();
    expect(getToken).not.toHaveBeenCalled();
    expect(fetchMock).toHaveBeenCalledTimes(1);
  });

it("refuses a browser session switch while the API token is pending", async () => {
  const pending = Promise.withResolvers<string>();
  getToken.mockResolvedValueOnce(jwt(USER_ID, BROWSER_ID)).mockImplementation(() => pending.promise);
  await show();
  await clickAuthorize();
  if (!clerk.session) throw new Error("Expected the browser session");
  clerk.session.id = OTHER_ID;
  await act(async () => { pending.resolve(jwt(USER_ID)); });
  await expectRefused();
  expect(fetchMock).toHaveBeenCalledTimes(1);
});

it("checks the displayed account again after real encryption completes", async () => {
  const reached = Promise.withResolvers<void>();
  const release = Promise.withResolvers<void>();
  const encrypt = crypto.subtle.encrypt.bind(crypto.subtle);
  vi.spyOn(crypto.subtle, ENCRYPT_METHOD).mockImplementation(async (...args) => {
    const ciphertext = await encrypt(...args);
    reached.resolve();
    await release.promise;
    return ciphertext;
  });
  await show();
  await clickAuthorize();
  await reached.promise;
  clerk.session = null;
  await act(async () => { release.resolve(); });
  await expectRefused();
  expect(fetchMock).toHaveBeenCalledTimes(1);
});

it.each([1, 2])("stops on failure obtaining token %i without sending approval", async ordinal => {
  if (ordinal === 2) getToken.mockResolvedValueOnce(jwt(USER_ID, BROWSER_ID));
  getToken.mockRejectedValue(new Error("Clerk unavailable"));
  await show();
  await clickAuthorize();
  await expectRefused();
  expect(screen.getByText(RESTART_TEXT)).toBeTruthy();
  expect(getToken).toHaveBeenCalledTimes(ordinal);
  expect(fetchMock).toHaveBeenCalledTimes(1);
});

it("does not send an approval after WebCrypto refuses encryption", async () => {
  await show();
  vi.spyOn(crypto.subtle, ENCRYPT_METHOD).mockRejectedValue(new Error("encryption unavailable"));
  await clickAuthorize();
  await expectRefused();
  expect(screen.getByText(RESTART_TEXT)).toBeTruthy();
  expect(getToken).toHaveBeenCalledTimes(2);
  expect(fetchMock).toHaveBeenCalledTimes(1);
});

it("requires a fresh sign-in after a redirected approval without sending its code", async () => {
  await show();
  const response = new Response(null);
  Object.defineProperty(response, REDIRECTED_PROPERTY, { value: true });
  fetchMock.mockResolvedValue(response);
  await clickAuthorize();
  await expectRefused();
  expect(fetchMock.mock.calls[1]?.[1]?.redirect).toBe("error");
  expect(fetchMock).toHaveBeenCalledTimes(2);
});

it("does not send a late successful approval after the page unmounts", async () => {
  const pending = Promise.withResolvers<Response>();
  await show();
  fetchMock.mockReturnValue(pending.promise);
  await clickAuthorize();
  await waitFor(() => expect(fetchMock).toHaveBeenCalledTimes(2));
  const signal = fetchMock.mock.calls[1]?.[1]?.signal;
  cleanup();
  expect(signal?.aborted).toBe(true);
  await act(async () => { pending.resolve(new Response(null)); });
  expect(submissions).toEqual([]);
});

it("does not submit after navigation unmounts between approval success and delivery", async () => {
  await show();
  const response = new Response(null);
  Object.defineProperty(response, REDIRECTED_PROPERTY, { get() { queueMicrotask(() => cleanup()); return false; } });
  fetchMock.mockResolvedValue(response);
  await clickAuthorize();
  await waitFor(() => { expect(fetchMock).toHaveBeenCalledTimes(2); });
  await act(async () => { await Promise.resolve(); });
  expect(submissions).toEqual([]);
  expect(document.body.querySelector(FORM_TAG)).toBeNull();
});

it("never retries an acknowledged-lost approval and permits only a fresh terminal session", async () => {
  const remoteApprovals: string[] = [];
  await show();
  fetchMock.mockImplementation((url: string) => {
    remoteApprovals.push(url);
    return Promise.reject(new Error("approval committed, acknowledgement lost"));
  });
  await clickAuthorize();
  await expectRefused();
  expect(screen.getByText(/Approval could not be confirmed/)).toBeTruthy();
  expect(remoteApprovals).toEqual([`/backend/v1/auth/sessions/${SESSION_ID}/approve`]);
  expect(screen.queryByLabelText(VERIFY_LABEL)).toBeNull();
  cleanup();
  await renderAuth();
  await screen.findByText("Sign-in unavailable");
  expect(remoteApprovals).toHaveLength(1);
  expect(getToken).toHaveBeenCalledTimes(2);
  expect(submissions).toEqual([]);
  cleanup();
  const nextSession = "0190b3c0-1111-7000-8000-000000000002";
  installHandoff({ ...handoff, sessionId: nextSession });
  fetchMock.mockResolvedValueOnce(Response.json(activeBody())).mockResolvedValue(new Response(null));
  await renderAuth(nextSession);
  await screen.findByLabelText(VERIFY_LABEL);
  await clickAuthorize();
  await waitFor(() => expect(submissions).toHaveLength(1));
  expect(submissions[0]?.session_id).toBe(nextSession);
  expect(remoteApprovals).toHaveLength(1);
});

it.each([RESOLVED_OUTCOME, "reject"])("ignores session lookup %s after page unmount", async outcome => {
  const pending = Promise.withResolvers<Response>();
  fetchMock.mockReturnValue(pending.promise);
  await renderAuth();
  cleanup();
  await act(async () => {
    if (outcome === RESOLVED_OUTCOME) pending.resolve(Response.json(activeBody()));
    else pending.reject(new Error("closed request"));
  });
  expect(submissions).toEqual([]);
  expect(getToken).not.toHaveBeenCalled();
  expect(readBrowserHandoff(SESSION_ID).kind).toBe("automatic");
});

it.each([AUTHORIZE, DENY])("cleans the callback form when %s submission throws", async decision => {
  await show();
  vi.spyOn(HTMLFormElement.prototype, "submit").mockImplementation(() => { throw new Error("navigation refused"); });
  await act(async () => { fireEvent.click(screen.getByRole(ROLE.button, { name: decision })); });
  await expectRefused();
  expect(document.body.querySelector(FORM_TAG)).toBeNull();
  expect(readBrowserHandoff(SESSION_ID).kind).toBe("invalid");
  expect(fetchMock).toHaveBeenCalledTimes(decision === DENY ? 1 : 2);
});

it("keeps authorization unavailable until Clerk has finished loading", async () => {
  auth.isLoaded = false;
  fetchMock.mockResolvedValue(Response.json(activeBody()));
  const mounted = await renderAuth();
  await waitFor(() => expect(fetchMock).toHaveBeenCalledTimes(1));
  expect(screen.queryByRole(ROLE.button, { name: AUTHORIZE })).toBeNull();
  auth.isLoaded = true;
  await act(async () => { mounted.refresh(); });
  await screen.findByLabelText(VERIFY_LABEL);
  expect(getToken).not.toHaveBeenCalled();
  expect(submissions).toEqual([]);
});

it.each([IDENTITY.signedOut, IDENTITY.missingUser, IDENTITY.missingIdentity, IDENTITY.missingSession])(
  "does not obtain a token with %s", async missing => {
    if (missing === IDENTITY.signedOut) auth.isSignedIn = false;
    if (missing === IDENTITY.missingUser) clerk.user = null;
    if (missing === IDENTITY.missingSession) clerk.session = null;
    if (missing === IDENTITY.missingIdentity && clerk.user) {
      clerk.user.primaryEmailAddress = null;
      clerk.user.fullName = null;
    }
    await show();
    await clickAuthorize();
    expect(getToken).not.toHaveBeenCalled();
    expect(submissions).toEqual([]);
    expect(fetchMock).toHaveBeenCalledTimes(1);
  });

it("shows the account name when the signed-in user has no primary email", async () => {
  if (!clerk.user) throw new Error("Expected the signed-in account");
  clerk.user.primaryEmailAddress = null;
  await show();
  expect(screen.getByText("Signing in as Person")).toBeTruthy();
  await act(async () => { fireEvent.click(screen.getByRole(ROLE.button, { name: DENY })); });
  expect(submissions).toEqual([{ state: handoff.state, session_id: SESSION_ID, status: "denied" }]);
});
