import { afterEach, beforeEach, describe, expect, it } from "bun:test";

import {
  createSignInTicket,
  ensureFixtureTenantReady,
  mintTokens,
  provisionUser,
  revokeMintedSessions,
  revokeSession,
} from "./clerk-admin.ts";
import { IS_TEST_FIXTURE_METADATA_KEY } from "./constants.ts";

const CLERK_SECRET = "fixture-clerk-secret";
const FIXTURE_EMAIL = "cli-fixture@example.test";
const FIXTURE_OWNER = "acceptance-e2e-suite";
const FIXTURE_ROLE = "regular";
const EXISTING_USER_ID = "user_existing";
const CREATED_USER_ID = "user_created";
const SESSION_ID = "session_fixture";
const MINTED_SESSION_ID = "session_minted";
const TEMPLATE_JWT = "template.jwt.value";
const COOKIE_JWT = "cookie.jwt.value";
const SIGN_IN_TICKET = "fixture-sign-in-ticket";
const REDACTED_VALUE = "[REDACTED]";
const MIN_EPHEMERAL_PASSWORD_LENGTH = 64;
const HTTP_OK = 200;
const HTTP_UNAUTHORIZED = 401;
const HTTP_FORBIDDEN = 403;
const HTTP_NOT_FOUND = 404;
const ACCEPTANCE_API_URL = "https://api.example.test";
const WEBHOOK_SECRET = `${["wh", "sec"].join("")}_${btoa("fixture-webhook-secret")}`;
const TENANT_ID = "tenant_fixture";

interface CapturedRequest {
  readonly url: string;
  readonly init: RequestInit | undefined;
}

let originalFetch: typeof globalThis.fetch;
let requests: CapturedRequest[];

function jsonResponse(value: unknown, status = HTTP_OK): Response {
  return new Response(JSON.stringify(value), {
    status,
    headers: { "Content-Type": "application/json" },
  });
}

function installFetch(
  responder: (request: CapturedRequest, index: number) => Response | Promise<Response>,
): void {
  globalThis.fetch = Object.assign(
    async (
      input: Parameters<typeof fetch>[0],
      init?: Parameters<typeof fetch>[1],
    ): Promise<Response> => {
      const request = { url: String(input), init };
      requests.push(request);
      return responder(request, requests.length - 1);
    },
    { preconnect: originalFetch.preconnect },
  );
}

function requestBody(request: CapturedRequest): Record<string, unknown> {
  expect(typeof request.init?.body).toBe("string");
  return JSON.parse(request.init?.body as string) as Record<string, unknown>;
}

beforeEach(() => {
  originalFetch = globalThis.fetch;
  requests = [];
});

afterEach(async () => {
  try {
    await revokeMintedSessions();
  } finally {
    globalThis.fetch = originalFetch;
  }
});

describe("CLI fixture Clerk identity ownership", () => {
  it("creates a missing owned user with an ephemeral credential", async () => {
    installFetch((_request, index) => index === 0
      ? jsonResponse([])
      : jsonResponse({
          id: CREATED_USER_ID,
          public_metadata: {
            [IS_TEST_FIXTURE_METADATA_KEY]: true,
            owner: FIXTURE_OWNER,
          },
        }));

    const user = await provisionUser(CLERK_SECRET, {
      email: FIXTURE_EMAIL,
      role: FIXTURE_ROLE,
    });

    expect(user.id).toBe(CREATED_USER_ID);
    expect(requests).toHaveLength(2);
    const body = requestBody(requests[1]!);
    expect(body.email_address).toEqual([FIXTURE_EMAIL]);
    expect(typeof body.password).toBe("string");
    expect((body.password as string).length).toBeGreaterThanOrEqual(MIN_EPHEMERAL_PASSWORD_LENGTH);
    expect(body.public_metadata).toEqual({
      [IS_TEST_FIXTURE_METADATA_KEY]: true,
      owner: FIXTURE_OWNER,
      role: FIXTURE_ROLE,
    });
  });

  it("reuses an existing identity only when suite ownership metadata matches", async () => {
    installFetch(() => jsonResponse([{
      id: EXISTING_USER_ID,
      public_metadata: {
        [IS_TEST_FIXTURE_METADATA_KEY]: true,
        owner: FIXTURE_OWNER,
      },
    }]));

    const user = await provisionUser(CLERK_SECRET, { email: FIXTURE_EMAIL });

    expect(user.id).toBe(EXISTING_USER_ID);
    expect(requests).toHaveLength(1);
  });

  it("waits for tenant metadata before exposing a fixture identity to the live lane", async () => {
    const previousTarget = process.env.AGENTSFLEET_ACCEPTANCE_TARGET;
    const previousWebhookSecret = process.env.CLERK_WEBHOOK_SECRET;
    process.env.AGENTSFLEET_ACCEPTANCE_TARGET = ACCEPTANCE_API_URL;
    process.env.CLERK_WEBHOOK_SECRET = WEBHOOK_SECRET;
    installFetch((request) => {
      if (request.url.includes("/users?")) {
        return jsonResponse([{
          id: EXISTING_USER_ID,
          public_metadata: {
            [IS_TEST_FIXTURE_METADATA_KEY]: true,
            owner: FIXTURE_OWNER,
            tenant_id: TENANT_ID,
          },
        }]);
      }
      if (request.url.endsWith(`/users/${EXISTING_USER_ID}`)) {
        return jsonResponse({
          id: EXISTING_USER_ID,
          public_metadata: { tenant_id: TENANT_ID },
        });
      }
      if (request.url.endsWith("/v1/auth/identity-events/clerk")) {
        return jsonResponse({ created: false });
      }
      return jsonResponse({}, HTTP_NOT_FOUND);
    });

    try {
      await expect(ensureFixtureTenantReady(CLERK_SECRET, { email: FIXTURE_EMAIL }))
        .resolves.toMatchObject({ id: EXISTING_USER_ID });
      expect(requests.some((request) =>
        request.url.endsWith("/v1/auth/identity-events/clerk")
      )).toBe(true);
    } finally {
      if (previousTarget === undefined) delete process.env.AGENTSFLEET_ACCEPTANCE_TARGET;
      else process.env.AGENTSFLEET_ACCEPTANCE_TARGET = previousTarget;
      if (previousWebhookSecret === undefined) delete process.env.CLERK_WEBHOOK_SECRET;
      else process.env.CLERK_WEBHOOK_SECRET = previousWebhookSecret;
    }
  });

  it("refuses an existing identity without the suite ownership marker", async () => {
    installFetch(() => jsonResponse([{
      id: EXISTING_USER_ID,
      public_metadata: {
        [IS_TEST_FIXTURE_METADATA_KEY]: false,
        owner: "somebody-else",
      },
    }]));

    await expect(
      provisionUser(CLERK_SECRET, { email: FIXTURE_EMAIL }),
    ).rejects.toThrow("fixture ownership mismatch");
    expect(requests).toHaveLength(1);
  });

  it("redacts the generated credential when Clerk echoes it in a create failure", async () => {
    let generatedPassword = "";
    installFetch((request, index) => {
      if (index === 0) return jsonResponse([]);
      generatedPassword = requestBody(request).password as string;
      return new Response(`invalid password ${generatedPassword}`, { status: HTTP_NOT_FOUND });
    });

    let caught: unknown;
    try {
      await provisionUser(CLERK_SECRET, { email: FIXTURE_EMAIL });
    } catch (error: unknown) {
      caught = error;
    }

    expect(caught).toBeInstanceOf(Error);
    expect((caught as Error).message).toContain(REDACTED_VALUE);
    expect((caught as Error).message).not.toContain(generatedPassword);
  });

  it("treats an already-absent session as revoked", async () => {
    installFetch(() => new Response("session not found", { status: HTTP_NOT_FOUND }));

    await expect(revokeSession(CLERK_SECRET, SESSION_ID)).resolves.toBeUndefined();
    expect(requests).toHaveLength(1);
  });

  it("does not report authorization failures as session revocation", async () => {
    installFetch((_request, index) => new Response(
      "authorization failed",
      { status: index === 0 ? HTTP_UNAUTHORIZED : HTTP_FORBIDDEN },
    ));

    await expect(revokeSession(CLERK_SECRET, SESSION_ID)).rejects.toThrow("401");
    await expect(revokeSession(CLERK_SECRET, SESSION_ID)).rejects.toThrow("403");
    expect(requests).toHaveLength(2);
  });

  it("revokes every session registered by token minting", async () => {
    installFetch((request) => {
      if (request.url.endsWith("/sessions")) {
        return jsonResponse({ id: MINTED_SESSION_ID });
      }
      if (request.url.endsWith("/tokens/api")) {
        return jsonResponse({ jwt: TEMPLATE_JWT });
      }
      if (request.url.endsWith("/tokens")) {
        return jsonResponse({ jwt: COOKIE_JWT });
      }
      if (request.url.endsWith("/revoke")) return jsonResponse({});
      return jsonResponse({}, HTTP_NOT_FOUND);
    });

    const minted = await mintTokens(CLERK_SECRET, EXISTING_USER_ID);
    expect(minted).toEqual({
      sessionId: MINTED_SESSION_ID,
      sessionJwt: TEMPLATE_JWT,
      cookieJwt: COOKIE_JWT,
    });

    await revokeMintedSessions();
    expect(requests.some((request) =>
      request.url.endsWith(`/sessions/${MINTED_SESSION_ID}/revoke`)
    )).toBe(true);
  });

  it("revokes a created session when one parallel token mint fails", async () => {
    installFetch((request) => {
      if (request.url.endsWith("/sessions")) return jsonResponse({ id: MINTED_SESSION_ID });
      if (request.url.endsWith("/tokens/api")) return jsonResponse({ jwt: TEMPLATE_JWT });
      if (request.url.endsWith("/tokens")) return new Response("token mint failed", { status: HTTP_FORBIDDEN });
      if (request.url.endsWith("/revoke")) return jsonResponse({});
      return jsonResponse({}, HTTP_NOT_FOUND);
    });

    await expect(mintTokens(CLERK_SECRET, EXISTING_USER_ID)).rejects.toThrow("403");
    expect(requests.some((request) =>
      request.url.endsWith(`/sessions/${MINTED_SESSION_ID}/revoke`)
    )).toBe(true);
  });

  it("creates a one-time browser sign-in ticket for the owned user", async () => {
    installFetch(() => jsonResponse({ token: SIGN_IN_TICKET }));

    await expect(createSignInTicket(CLERK_SECRET, EXISTING_USER_ID))
      .resolves.toBe(SIGN_IN_TICKET);
    expect(requests[0]?.url).toContain("/sign_in_tokens");
    expect(requestBody(requests[0]!)).toEqual({
      user_id: EXISTING_USER_ID,
      expires_in_seconds: 300,
    });
  });
});
