import { describe, it, expect } from "vitest";
import {
  SECRET_DATA_EMPTY_OBJECT,
  SECRET_DATA_NOT_OBJECT,
  jsonParseErrorMessage,
  parseSecretDataObject,
} from "../app/(dashboard)/secrets/lib/secret-data";

// credential-data is the shared credential-body contract used by both
// EditCredentialDialog (JSON edit) and the credential write paths. The
// field-builder Add form no longer round-trips raw JSON, so these branches are
// covered directly against the pure function rather than through a component.

describe("jsonParseErrorMessage", () => {
  it("returns the message of a thrown Error (the JSON.parse SyntaxError path)", () => {
    expect(jsonParseErrorMessage(new SyntaxError("Unexpected token x"))).toBe("Unexpected token x");
  });

  it("falls back to a fixed label for a non-Error throw value", () => {
    expect(jsonParseErrorMessage("not-an-error")).toBe("Invalid JSON");
  });
});

describe("parseSecretDataObject", () => {
  const REQUIRED = "Data is required";

  it("rejects empty/whitespace input with the caller's required message", () => {
    expect(parseSecretDataObject("   ", REQUIRED)).toEqual({ ok: false, message: REQUIRED });
  });

  it("rejects malformed JSON with an Invalid JSON message", () => {
    const result = parseSecretDataObject("{not json", REQUIRED);
    expect(result.ok).toBe(false);
    expect((result as { ok: false; message: string }).message).toMatch(/^Invalid JSON:/);
  });

  it("rejects an array (must be an object)", () => {
    expect(parseSecretDataObject("[1,2,3]", REQUIRED)).toEqual({
      ok: false,
      message: SECRET_DATA_NOT_OBJECT,
    });
  });

  it("rejects a scalar (must be an object)", () => {
    expect(parseSecretDataObject("42", REQUIRED)).toEqual({
      ok: false,
      message: SECRET_DATA_NOT_OBJECT,
    });
  });

  it("rejects null (must be an object)", () => {
    expect(parseSecretDataObject("null", REQUIRED)).toEqual({
      ok: false,
      message: SECRET_DATA_NOT_OBJECT,
    });
  });

  it("rejects an empty object (must have at least one field)", () => {
    expect(parseSecretDataObject("{}", REQUIRED)).toEqual({
      ok: false,
      message: SECRET_DATA_EMPTY_OBJECT,
    });
  });

  it("accepts a non-empty object", () => {
    expect(parseSecretDataObject('{"host":"x","port":1}', REQUIRED)).toEqual({
      ok: true,
      data: { host: "x", port: 1 },
    });
  });
});
