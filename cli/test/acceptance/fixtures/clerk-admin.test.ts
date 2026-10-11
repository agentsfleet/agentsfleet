import { afterEach, beforeEach, describe, expect, it } from "bun:test";

import { provisionUser } from "./clerk-admin.ts";
import { IS_TEST_FIXTURE_METADATA_KEY } from "./constants.ts";

// The two harness checks that guard something outside the harness: a real
// dev-tenant user the suite does not own, and the generated password in CI
// logs. The fixture's other behaviour shows up as live-lane failures.
const CLERK_SECRET = "fixture-clerk-secret";
const FIXTURE_EMAIL = "cli-fixture@example.test";
const EXISTING_USER_ID = "user_existing";
const REDACTED_VALUE = "[REDACTED]";
const HTTP_OK = 200;
const HTTP_NOT_FOUND = 404;

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

afterEach(() => {
  globalThis.fetch = originalFetch;
});

describe("CLI fixture Clerk identity safety", () => {
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
});
