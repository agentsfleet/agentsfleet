import { describe, it, expect } from "vitest";
import {
  SECRET_DATA_EMPTY_OBJECT,
  SECRET_DATA_MALFORMED_JSON,
  SECRET_DATA_NOT_OBJECT,
  jsonParseErrorMessage,
  parseSecretDataObject,
} from "../app/(dashboard)/secrets/lib/secret-data";

// credential-data is the shared credential-body contract used by both
// EditCredentialDialog (JSON edit) and the credential write paths. The
// field-builder Add form no longer round-trips raw JSON, so these branches are
// covered directly against the pure function rather than through a component.

describe("jsonParseErrorMessage", () => {
  it("returns the fixed friendly message for a thrown Error, never the native SyntaxError.message", () => {
    expect(jsonParseErrorMessage(new SyntaxError("Unexpected token x"))).toBe(SECRET_DATA_MALFORMED_JSON);
  });

  it("returns the same fixed message for a non-Error throw value", () => {
    expect(jsonParseErrorMessage("not-an-error")).toBe(SECRET_DATA_MALFORMED_JSON);
  });
});

describe("parseSecretDataObject", () => {
  const REQUIRED = "Data is required";

  it("rejects empty/whitespace input with the caller's required message", () => {
    expect(parseSecretDataObject("   ", REQUIRED)).toEqual({ ok: false, message: REQUIRED });
  });

  it("rejects malformed JSON with the fixed friendly message, never the native SyntaxError", () => {
    expect(parseSecretDataObject("{not json", REQUIRED)).toEqual({
      ok: false,
      message: SECRET_DATA_MALFORMED_JSON,
    });
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
