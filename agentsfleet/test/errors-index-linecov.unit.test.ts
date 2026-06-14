// Line-coverage backfill for src/errors/index.ts. The import region at the
// top of the module (lines 13-15) is the only span the aggregate coverage
// run flags uncovered for this file. Line 13 (`import { Data }`) backs every
// `Data.TaggedError(...)` class declaration below it; constructing each
// concrete error class — and re-reading the `export *` surface from auth.ts
// referenced by the type-only import on line 14 — drives that runtime import
// to execution. These tests assert real behaviour (tag, exit code, rendered
// message) rather than touching lines blindly.

import { describe, expect, test } from "bun:test";
import {
  AuthError,
  ConfigError,
  EXIT_CODE,
  InterruptedError,
  NetworkError,
  ServerError,
  UnexpectedError,
  ValidationError,
  type CliError,
} from "../src/errors/index.ts";

const detail = "boom";
const suggestion = "retry";
const suggestionPrefix = "Suggestion: ";

// Each row exercises a concrete `Data.TaggedError` class declared right under
// the `import { Data } from "effect"` statement (errors/index.ts:13). Building
// the instance runs that runtime import and the class's `message` getter.
const baseFields = { detail, suggestion } as const;

describe("errors/index runtime import surface", () => {
  const cases: ReadonlyArray<{ name: CliError["_tag"]; build: () => CliError }> = [
    {
      name: "AuthError",
      build: () => new AuthError({ ...baseFields, code: "UZ-AUTH-001" }),
    },
    {
      name: "NetworkError",
      build: () => new NetworkError({ ...baseFields, url: "https://x.test" }),
    },
    {
      name: "ServerError",
      build: () =>
        new ServerError({
          ...baseFields,
          code: "UZ-SRV-503",
          status: 503,
          requestId: "req-import",
        }),
    },
    { name: "ValidationError", build: () => new ValidationError(baseFields) },
    { name: "ConfigError", build: () => new ConfigError(baseFields) },
    { name: "UnexpectedError", build: () => new UnexpectedError(baseFields) },
  ];

  for (const { name, build } of cases) {
    test(`${name} construction renders detail + suggestion via Data.TaggedError`, () => {
      const err = build();
      // Tag proves the Data.TaggedError factory (backed by errors/index.ts:13)
      // produced the right discriminant.
      expect(err._tag).toBe(name);
      // message getter concatenates detail + suggestion — real behaviour, not
      // a bare construction.
      expect(err.message).toBe(`${detail}\n  ${suggestionPrefix}${suggestion}`);
    });
  }
});

describe("errors/index EXIT_CODE wiring for the core variants", () => {
  test("each core variant has a positive numeric exit code", () => {
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

  test("auth-flow re-export (export * from ./auth.ts) is reachable through index", () => {
    // InterruptedError originates in auth.ts and is surfaced via the
    // `export * from "./auth.ts"` line that sits adjacent to the flagged
    // import region. Constructing it through the index entry point proves the
    // re-export wiring and its conventional SIGINT exit code.
    const err = new InterruptedError(baseFields);
    expect(err._tag).toBe("InterruptedError");
    expect(EXIT_CODE.InterruptedError).toBe(130);
    expect(err.message).toContain(detail);
  });
});
