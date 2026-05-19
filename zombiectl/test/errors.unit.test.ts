import { describe, test, expect } from "bun:test";
import {
  AuthError,
  ConfigError,
  EXIT_CODE,
  NetworkError,
  ServerError,
  UnexpectedError,
  ValidationError,
  type CliError,
} from "../src/errors/index.ts";

const detail = "thing broke";
const suggestion = "try again";

describe("CliError variants", () => {
  test("AuthError carries _tag, code, and renders detail + suggestion in message", () => {
    const err = new AuthError({ detail, suggestion, code: "UZ-AUTH-001" });
    expect(err._tag).toBe("AuthError");
    expect(err.code).toBe("UZ-AUTH-001");
    expect(err.message).toContain(detail);
    expect(err.message).toContain(suggestion);
  });
  test("NetworkError carries url + tag", () => {
    const err = new NetworkError({ detail, suggestion, url: "https://x.test" });
    expect(err._tag).toBe("NetworkError");
    expect(err.url).toBe("https://x.test");
    expect(err.message).toContain("Suggestion:");
  });
  test("ServerError carries code/status/requestId", () => {
    const err = new ServerError({
      detail,
      suggestion,
      code: "UZ-AUTH-002",
      status: 401,
      requestId: "req-1",
    });
    expect(err._tag).toBe("ServerError");
    expect(err.status).toBe(401);
    expect(err.requestId).toBe("req-1");
  });
  test("ValidationError shape", () => {
    const err = new ValidationError({ detail, suggestion });
    expect(err._tag).toBe("ValidationError");
    expect(err.message).toContain(detail);
  });
  test("ConfigError shape", () => {
    const err = new ConfigError({ detail, suggestion });
    expect(err._tag).toBe("ConfigError");
  });
  test("UnexpectedError shape", () => {
    const err = new UnexpectedError({ detail, suggestion });
    expect(err._tag).toBe("UnexpectedError");
  });
});

describe("EXIT_CODE map", () => {
  test("every CliError._tag has a numeric exit code", () => {
    const tags: ReadonlyArray<CliError["_tag"]> = [
      "AuthError",
      "NetworkError",
      "ServerError",
      "ValidationError",
      "ConfigError",
      "UnexpectedError",
    ];
    for (const tag of tags) {
      expect(typeof EXIT_CODE[tag]).toBe("number");
      expect(EXIT_CODE[tag]).toBeGreaterThan(0);
    }
  });
  test("AuthError and UnexpectedError both map to 1 (cli convention)", () => {
    expect(EXIT_CODE.AuthError).toBe(1);
    expect(EXIT_CODE.UnexpectedError).toBe(1);
  });
  test("NetworkError, ServerError, ValidationError, ConfigError have distinct codes", () => {
    const codes = new Set([
      EXIT_CODE.NetworkError,
      EXIT_CODE.ServerError,
      EXIT_CODE.ValidationError,
      EXIT_CODE.ConfigError,
    ]);
    expect(codes.size).toBe(4);
  });
});
