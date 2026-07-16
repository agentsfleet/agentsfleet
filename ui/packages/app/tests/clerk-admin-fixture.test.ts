import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  createSignInTicket,
  findUserIdByEmail,
} from "./e2e/acceptance/fixtures/clerk-admin";

const FIXTURE_EMAIL = "operator-fixture@mailinator.com";
const FIXTURE_USER_ID = "user_fixture";
const CLERK_SECRET = "sk_test_fixture";
const RETRY_AFTER_SECONDS = "1";
const RETRY_AFTER_MILLISECONDS = 1000;
const NETWORK_BACKOFF_MILLISECONDS = 500;
const FIND_USER_ERROR_PREFIX =
  "Clerk GET /users?email_address=operator-fixture%40mailinator.com";

function userResponse(): Response {
  return Response.json([
    {
      id: FIXTURE_USER_ID,
      email_addresses: [{ email_address: FIXTURE_EMAIL }],
    },
  ]);
}

describe("Clerk administration requests", () => {
  beforeEach(() => {
    vi.useFakeTimers();
    vi.stubEnv("CLERK_SECRET_KEY", CLERK_SECRET);
    vi.spyOn(Math, "random").mockReturnValue(0);
    vi.spyOn(process.stderr, "write").mockImplementation(() => true);
  });

  afterEach(() => {
    vi.useRealTimers();
    vi.unstubAllEnvs();
    vi.unstubAllGlobals();
    vi.restoreAllMocks();
  });

  it("should honor Retry-After before retrying a rate-limited lookup", async () => {
    const fetchMock = vi
      .fn()
      .mockResolvedValueOnce(
        new Response("rate limited", {
          status: 429,
          headers: { "Retry-After": RETRY_AFTER_SECONDS },
        }),
      )
      .mockResolvedValueOnce(userResponse());
    vi.stubGlobal("fetch", fetchMock);

    const lookup = findUserIdByEmail(FIXTURE_EMAIL);
    const resolution = expect(lookup).resolves.toBe(FIXTURE_USER_ID);
    await vi.advanceTimersByTimeAsync(RETRY_AFTER_MILLISECONDS - 1);
    expect(fetchMock).toHaveBeenCalledTimes(1);
    await vi.advanceTimersByTimeAsync(1);

    await resolution;
    expect(fetchMock).toHaveBeenCalledTimes(2);
  });

  it("should retry a connection reset before returning the fixture user", async () => {
    const fetchMock = vi
      .fn()
      .mockRejectedValueOnce(new TypeError("read ECONNRESET"))
      .mockResolvedValueOnce(userResponse());
    vi.stubGlobal("fetch", fetchMock);

    const lookup = findUserIdByEmail(FIXTURE_EMAIL);
    const resolution = expect(lookup).resolves.toBe(FIXTURE_USER_ID);
    await vi.advanceTimersByTimeAsync(NETWORK_BACKOFF_MILLISECONDS);

    await resolution;
    expect(fetchMock).toHaveBeenCalledTimes(2);
  });

  it("should fail an ordinary client error without retrying", async () => {
    const fetchMock = vi.fn().mockResolvedValue(new Response("bad request", { status: 400 }));
    vi.stubGlobal("fetch", fetchMock);

    await expect(findUserIdByEmail(FIXTURE_EMAIL)).rejects.toThrow(
      `${FIND_USER_ERROR_PREFIX} → 400: bad request`,
    );
    expect(fetchMock).toHaveBeenCalledTimes(1);
  });

  it("should stop after four unavailable responses", async () => {
    const fetchMock = vi
      .fn()
      .mockResolvedValue(new Response("unavailable", { status: 503 }));
    vi.stubGlobal("fetch", fetchMock);

    const lookup = findUserIdByEmail(FIXTURE_EMAIL);
    const rejection = expect(lookup).rejects.toThrow(
      `${FIND_USER_ERROR_PREFIX} → 503: unavailable`,
    );
    await vi.runAllTimersAsync();

    await rejection;
    expect(fetchMock).toHaveBeenCalledTimes(4);
  });

  it("should report the final network error after four failed attempts", async () => {
    const fetchMock = vi.fn().mockRejectedValue(new TypeError("network down"));
    vi.stubGlobal("fetch", fetchMock);

    const lookup = findUserIdByEmail(FIXTURE_EMAIL);
    const rejection = expect(lookup).rejects.toThrow(
      "Clerk GET request failed after 4 attempts: network down",
    );
    await vi.runAllTimersAsync();

    await rejection;
    expect(fetchMock).toHaveBeenCalledTimes(4);
  });

  it("should not replay a non-idempotent request after a connection reset", async () => {
    const fetchMock = vi.fn().mockRejectedValue(new TypeError("read ECONNRESET"));
    vi.stubGlobal("fetch", fetchMock);

    await expect(createSignInTicket(FIXTURE_USER_ID)).rejects.toThrow(
      "Clerk POST request failed after 1 attempt: read ECONNRESET",
    );
    expect(fetchMock).toHaveBeenCalledTimes(1);
  });
});
