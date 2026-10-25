import { expect, it } from "vitest";
import { act, cleanup, fireEvent, screen, waitFor } from "@testing-library/react";
import { deriveSharedKey } from "@/lib/auth/cli-flow";
import { matchingCode } from "@/lib/auth/cli-loopback";
import {
  AUTHORIZE, BROWSER_ID, DENY, JSON_TYPE, KEY_FORMAT, OTHER_ID, ROLE, SESSION_ID, USER_ID,
  activeBody, clerk, clickAuthorize, expectRefused, fetchMock, getToken, handoff, jwt,
  renderAuth, show, submissions, terminal,
} from "./helpers/cli-loopback-fixture";

const APPROVED_STATUS = "approved";

it("shows the matching code and account before releasing only the encrypted approval to its local terminal", async () => {
  await show();
  expect(screen.getByText(/Signing in as person@example.test/)).toBeTruthy();
  expect(screen.getByLabelText("Verification code").textContent).toBe(await matchingCode(handoff));
  expect(getToken).not.toHaveBeenCalled();
  await clickAuthorize();
  await waitFor(() => expect(submissions).toHaveLength(1));
  const request = fetchMock.mock.calls[1]?.[1] as RequestInit;
  expect(request.headers).toEqual({ "Content-Type": JSON_TYPE, Authorization: `Bearer ${jwt(USER_ID, BROWSER_ID)}` });
  if (typeof request.body !== "string") throw new Error("Expected a JSON request body");
  const body = JSON.parse(request.body) as Record<string, string>;
  expect(request.body).not.toContain(jwt(USER_ID));
  expect(submissions[0]).toEqual({ state: handoff.state, session_id: SESSION_ID,
    status: APPROVED_STATUS, verification_code: body.verification_code });
  expect(body.verification_code).toMatch(/^\d{6}$/);
  const key = await deriveSharedKey(terminal.privateKey, body.dashboard_public_key!);
  const plaintext = await crypto.subtle.decrypt({ name: "AES-GCM", iv: Buffer.from(body.nonce!, KEY_FORMAT) },
    key, Buffer.from(body.ciphertext!, KEY_FORMAT));
  expect(new TextDecoder().decode(plaintext)).toBe(jwt(USER_ID));
});

it("denies once without obtaining tokens or approving at the API", async () => {
  await show();
  await act(async () => {
    fireEvent.click(screen.getByRole(ROLE.button, { name: DENY }));
    fireEvent.click(screen.getByRole(ROLE.button, { name: AUTHORIZE }));
  });
  expect(submissions).toEqual([{ state: handoff.state, session_id: SESSION_ID, status: "denied" }]);
  expect(getToken).not.toHaveBeenCalled();
  expect(fetchMock).toHaveBeenCalledTimes(1);
});

it("arbitrates repeated approval and denial clicks before React rerenders", async () => {
  await show();
  await act(async () => {
    fireEvent.click(screen.getByRole(ROLE.button, { name: AUTHORIZE }));
    fireEvent.click(screen.getByRole(ROLE.button, { name: AUTHORIZE }));
    fireEvent.click(screen.getByRole(ROLE.button, { name: DENY }));
  });
  await waitFor(() => expect(submissions).toHaveLength(1));
  expect(submissions[0]?.status).toBe(APPROVED_STATUS);
  expect(fetchMock).toHaveBeenCalledTimes(2);
});

it.each([
  { cli_public_key: "substituted-key" },
  { expires_at: new Date(0).toISOString() },
  { expires_at: "invalid" },
  { status: "verification_pending" },
])("refuses an unbound or unavailable API session %j", async (overrides) => {
  fetchMock.mockResolvedValue(Response.json(activeBody(overrides)));
  await renderAuth();
  await expectRefused();
  expect(getToken).not.toHaveBeenCalled();
});

it.each([
  [jwt(OTHER_ID, BROWSER_ID), jwt(USER_ID)],
  [jwt(USER_ID, OTHER_ID), jwt(USER_ID)],
  [jwt(USER_ID, BROWSER_ID), jwt(OTHER_ID)],
  [null, jwt(USER_ID)],
  [jwt(USER_ID, BROWSER_ID), null],
])("refuses tokens for a different account, session or missing identity", async (sessionToken, token) => {
  getToken.mockImplementation(async (options?: { template: string }) => options ? token : sessionToken);
  await show();
  await clickAuthorize();
  await expectRefused();
  expect(fetchMock).toHaveBeenCalledTimes(1);
});

it("refuses an account switch during token acquisition", async () => {
  const pending = Promise.withResolvers<string>();
  getToken.mockImplementation(() => pending.promise);
  await show();
  await clickAuthorize();
  if (!clerk.user) throw new Error("Expected the signed-in account");
  clerk.user.id = OTHER_ID;
  await act(async () => { pending.resolve(jwt(USER_ID, BROWSER_ID)); });
  await expectRefused();
  expect(fetchMock).toHaveBeenCalledTimes(1);
});

it.each([401, 409, 410, 503])("does not release a verification code after approval refusal %i", async (status) => {
  await show();
  fetchMock.mockResolvedValue(new Response(null, { status }));
  await clickAuthorize();
  await expectRefused();
  expect(fetchMock).toHaveBeenCalledTimes(2);
});

it("requires a new session after a lost approval reply without retrying or releasing its code", async () => {
  await show();
  fetchMock.mockRejectedValue(new Error("connection lost"));
  await clickAuthorize();
  await expectRefused();
  expect(screen.getByText(/Approval could not be confirmed/)).toBeTruthy();
  expect(fetchMock).toHaveBeenCalledTimes(2);
});

it("does not deliver an approval from a component that unmounts while tokens are pending", async () => {
  const pending = Promise.withResolvers<string>();
  getToken.mockImplementation(() => pending.promise);
  await show();
  await clickAuthorize();
  cleanup();
  await act(async () => { pending.resolve(jwt(USER_ID, BROWSER_ID)); });
  expect(submissions).toEqual([]);
  expect(fetchMock).toHaveBeenCalledTimes(1);
});
