// D25 — pure detection function. Each test pins one rule of the
// scanner; together they document the wire-shape coverage matrix.

import { describe, expect, test } from "bun:test";
import { TOKEN_LEAK_WARNING, detectTokenInArgv } from "../src/lib/argv-redact.ts";

describe("detectTokenInArgv", () => {
  test("empty argv returns null", () => {
    expect(detectTokenInArgv([])).toBeNull();
  });

  test("argv without --token returns null", () => {
    expect(detectTokenInArgv(["login", "--api", "https://api.test"])).toBeNull();
  });

  test("--token=<value> trips the warning", () => {
    expect(detectTokenInArgv(["agent", "list", "--token=secret"])).toBe(TOKEN_LEAK_WARNING);
  });

  test("--token <value> (two-arg form) trips the warning", () => {
    expect(detectTokenInArgv(["billing", "show", "--token", "secret"])).toBe(TOKEN_LEAK_WARNING);
  });

  test("--token= with empty value is harmless (nothing to leak)", () => {
    expect(detectTokenInArgv(["doctor", "--token="])).toBeNull();
  });

  test("--token at end of argv with no value is harmless", () => {
    expect(detectTokenInArgv(["doctor", "--token"])).toBeNull();
  });

  test("--token followed by another flag is harmless (no value to capture)", () => {
    expect(detectTokenInArgv(["doctor", "--token", "--json"])).toBeNull();
  });

  test("end-of-options `--` stops the scan — `--token` as positional doesn't fire", () => {
    expect(detectTokenInArgv(["agent", "steer", "z1", "--", "--token=value-as-positional"])).toBeNull();
  });

  test("multiple flags: --token=<value> in the middle still fires", () => {
    expect(detectTokenInArgv(["--json", "--token=abc", "doctor"])).toBe(TOKEN_LEAK_WARNING);
  });
});
