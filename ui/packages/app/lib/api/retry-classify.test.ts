import { describe, expect, it } from "vitest";
import { fetchFailed } from "@/tests/helpers/fetch-failed";
import { ApiError, HTTP_STATUS_REQUEST_TIMEOUT, RETRY_CODE_TIMEOUT } from "./errors";
import { ClassifiedFailure, FAILURE_KIND, PROVENANCE, TRANSIENT_STATUSES, classifyFailure } from "./retry-classify";

const RETRY_AFTER_MS = 5000;
const NEVER_SENT = false;
const HEADERS_ARRIVED = true;


describe("every status maps to its own class", () => {
  it.each([
    [HTTP_STATUS_REQUEST_TIMEOUT, FAILURE_KIND.TIMEOUT, PROVENANCE.ANSWERED],
    [425, FAILURE_KIND.EARLY, PROVENANCE.ANSWERED],
    [429, FAILURE_KIND.RATE, PROVENANCE.ANSWERED],
    [502, FAILURE_KIND.SERVER, PROVENANCE.POST_SEND],
    [503, FAILURE_KIND.SERVER, PROVENANCE.POST_SEND],
    [504, FAILURE_KIND.SERVER, PROVENANCE.POST_SEND],
  ])("%i is %s, %s", (status, kind, provenance) => {
    const err = new ApiError("svc", status, "X");
    const failure = classifyFailure(err, NEVER_SENT);
    expect(failure).toBeInstanceOf(ClassifiedFailure);
    expect(failure).toMatchObject({ kind, provenance, status, retryAfterMs: null, cause: err });
  });

  it("the transient set the policy advertises is exactly this table", () => {
    expect([...TRANSIENT_STATUSES].sort((a, b) => a - b)).toEqual([408, 425, 429, 502, 503, 504]);
  });

  it("the transport's own timeout is a timeout the server may still be working on", () => {
    const err = new ApiError("timed out", HTTP_STATUS_REQUEST_TIMEOUT, RETRY_CODE_TIMEOUT);
    expect(classifyFailure(err, NEVER_SENT)).toMatchObject({
      kind: FAILURE_KIND.TIMEOUT,
      provenance: PROVENANCE.POST_SEND,
      status: HTTP_STATUS_REQUEST_TIMEOUT,
    });
  });

  it("a Retry-After rides the classification", () => {
    const err = new ApiError("slow", 429, "X", undefined, RETRY_AFTER_MS);
    expect(classifyFailure(err, NEVER_SENT).retryAfterMs).toBe(RETRY_AFTER_MS);
  });

  it.each([400, 401, 404, 500])("%i is fatal: the server answered and will not be asked again", (status) => {
    const failure = classifyFailure(new ApiError("no", status, "X"), NEVER_SENT);
    expect(failure).toMatchObject({ kind: FAILURE_KIND.FATAL, provenance: PROVENANCE.ANSWERED, status });
  });
});

describe("a network failure's provenance is read from its cause", () => {
  it.each(["ECONNREFUSED", "ENOTFOUND", "EAI_AGAIN", "UND_ERR_CONNECT_TIMEOUT"])(
    "%s proves the request never left",
    (code) => {
      expect(classifyFailure(fetchFailed(code), NEVER_SENT)).toMatchObject({
        kind: FAILURE_KIND.NETWORK,
        provenance: PROVENANCE.PRE_SEND,
        status: undefined,
        retryAfterMs: null,
      });
    },
  );

  it.each(["ECONNRESET", "EPIPE", "ETIMEDOUT", "UND_ERR_HEADERS_TIMEOUT", "UND_ERR_BODY_TIMEOUT", "EPERM"])(
    "%s arrived after the request was on the wire",
    (code) => {
      expect(classifyFailure(fetchFailed(code), NEVER_SENT)).toMatchObject({
        kind: FAILURE_KIND.NETWORK,
        provenance: PROVENANCE.POST_SEND,
      });
    },
  );

  it("a message without a cause is treated as possibly sent", () => {
    // The browser-shaped rejection: no cause, and a message this module never reads.
    expect(classifyFailure(new TypeError("Failed to fetch"), NEVER_SENT)).toMatchObject({
      kind: FAILURE_KIND.NETWORK,
      provenance: PROVENANCE.POST_SEND,
    });
    // A cause that carries no string code says nothing either.
    for (const cause of ["socket hung up", null, { errno: -54 }, { code: 54 }]) {
      expect(classifyFailure(new TypeError("fetch failed", { cause }), NEVER_SENT).provenance).toBe(
        PROVENANCE.POST_SEND,
      );
    }
  });

  it("headers that already arrived make any code post-send", () => {
    expect(classifyFailure(fetchFailed("ECONNREFUSED"), HEADERS_ARRIVED).provenance).toBe(PROVENANCE.POST_SEND);
  });
});

describe("what the classifier does not touch", () => {
  it("a failure already classified passes through untouched", () => {
    const once = classifyFailure(new ApiError("svc", 503, "X"), NEVER_SENT);
    expect(classifyFailure(once, HEADERS_ARRIVED)).toBe(once);
  });

  it("anything else is fatal and keeps what was thrown as its cause", () => {
    for (const thrown of ["a string", null, new RangeError("not a transport error")]) {
      expect(classifyFailure(thrown, NEVER_SENT)).toMatchObject({ kind: FAILURE_KIND.FATAL, cause: thrown });
    }
  });

  it("a classified failure is a tagged Error the policy can match on", () => {
    const failure = classifyFailure(new TypeError("Failed to fetch"), NEVER_SENT);
    expect(failure).toBeInstanceOf(Error);
    expect(failure._tag).toBe("ClassifiedFailure");
  });
});
